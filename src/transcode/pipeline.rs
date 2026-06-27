//! The shared transcode loop: demux → decode → (filter) → encode → mux, with audio
//! stream-copied. Tiers 1 and 2 both drive this.

use super::config::VideoConfig;
use super::progress::{Progress, TranscodeSummary};
use crate::codec::encoder::VideoEncoder;
use crate::codec::decoder::Decoder;
use crate::error::{Error, Result};
use crate::filter::{FilterChain, VideoFilter};
use crate::format::{MediaReader, MediaWriter};
use crate::frame::Frame;
use crate::types::codec::VideoCodec;
use crate::types::rational::{Framerate, Rational};
use crate::types::stream_kind::StreamKind;
use std::time::Instant;

/// The resolved set of options a transcode runs with.
pub(crate) struct TranscodeOptions {
    pub input: String,
    pub output: String,
    pub video: Option<VideoConfig>,
    pub drop_video: bool,
    pub drop_audio: bool,
    pub trim: Option<(f64, f64)>,
    pub filter: FilterChain,
}

/// Convert a time in seconds to a timestamp in `tb`.
fn secs_to_ts(secs: f64, tb: Rational) -> i64 {
    if tb.num == 0 {
        0
    } else {
        (secs * tb.den as f64 / tb.num as f64).round() as i64
    }
}

/// Encode one (already filtered) frame and mux the resulting packets.
fn encode_and_mux(
    encoder: &mut VideoEncoder,
    writer: &mut MediaWriter,
    out_idx: usize,
    mut frame: Frame,
    trim_start_ts: i64,
    frames: &mut u64,
) -> Result<()> {
    let ts = frame
        .best_effort_timestamp()
        .or_else(|| frame.pts())
        .unwrap_or(0);
    frame.set_pts(ts - trim_start_ts);
    for pkt in encoder.encode(&frame)? {
        let mut pkt = pkt?;
        pkt.set_stream_index(out_idx);
        writer.write_packet(&mut pkt)?;
    }
    *frames += 1;
    Ok(())
}

/// Run a frame (from decode or decoder-flush) through the optional filter, then encode it.
/// The filter graph is built eagerly during setup, so `vfilter` is already populated when
/// filtering is in effect.
fn process_video_frame(
    frame: Frame,
    vfilter: &mut Option<VideoFilter>,
    encoder: &mut VideoEncoder,
    writer: &mut MediaWriter,
    out_idx: usize,
    trim_start_ts: i64,
    frames: &mut u64,
) -> Result<()> {
    match vfilter {
        None => encode_and_mux(encoder, writer, out_idx, frame, trim_start_ts, frames),
        Some(vf) => {
            for out in vf.filter(frame)? {
                encode_and_mux(encoder, writer, out_idx, out, trim_start_ts, frames)?;
            }
            Ok(())
        }
    }
}

/// A decoded frame's presentation time, in seconds, using the source time base.
fn frame_secs(frame: &Frame, tb_f64: f64) -> f64 {
    let ts = frame
        .best_effort_timestamp()
        .or_else(|| frame.pts())
        .unwrap_or(0);
    ts as f64 * tb_f64
}

pub(crate) fn run(
    opts: &TranscodeOptions,
    mut on_progress: impl FnMut(Progress),
) -> Result<TranscodeSummary> {
    let mut reader = MediaReader::open(&opts.input)?;
    let total_secs = reader.duration_secs();

    let video_idx = if opts.drop_video {
        None
    } else {
        reader.best_stream(StreamKind::Video).ok()
    };
    let audio_idx = if opts.drop_audio {
        None
    } else {
        reader.best_stream(StreamKind::Audio).ok()
    };
    if video_idx.is_none() && audio_idx.is_none() {
        return Err(Error::InvalidConfig(
            "nothing to transcode (both video and audio dropped or absent)",
        ));
    }

    let mut writer = MediaWriter::create(&opts.output)?;

    // --- video setup -------------------------------------------------------------------
    let mut decoder: Option<Decoder> = None;
    let mut encoder: Option<VideoEncoder> = None;
    let mut vfilter: Option<VideoFilter> = None;
    let mut out_vidx = 0usize;
    let mut v_tb = Rational::new(1, 1);

    if let Some(vidx) = video_idx {
        let in_tb = reader.stream_time_base(vidx)?;
        v_tb = in_tb;
        let avg_fr = reader.stream_avg_frame_rate(vidx)?;
        let dec = reader.stream(vidx).decoder()?;

        let cfg = opts.video.clone();
        let codec = cfg.as_ref().map(|c| c.codec).unwrap_or(VideoCodec::H264);
        let (in_w, in_h) = (dec.width() as i32, dec.height() as i32);
        let in_pix = dec.pixel_format();
        let (tw, th) = cfg
            .as_ref()
            .and_then(|c| c.resolution)
            .map(|(w, h)| (w as i32, h as i32))
            .unwrap_or((in_w, in_h));

        // Compose the effective filter chain: user filters, plus an auto scale when the
        // requested resolution differs from the input.
        let mut chain = opts.filter.clone();
        if (tw, th) != (in_w, in_h) {
            let mut stages = vec![format!("scale={tw}:{th}")];
            if !opts.filter.is_empty() {
                stages.push(opts.filter.description());
            }
            chain = FilterChain::raw(stages.join(","));
        }

        // Build the filter graph eagerly so the encoder is sized to the graph's actual
        // output, which is the only correct dimension/format even for arbitrary user filters.
        let (enc_w, enc_h, enc_pix) = if chain.is_empty() {
            (in_w, in_h, in_pix)
        } else {
            let f = VideoFilter::new(in_w, in_h, in_pix, in_tb, Rational::new(1, 1), &chain)?;
            let dims = (f.output_width(), f.output_height(), f.output_pixel_format());
            vfilter = Some(f);
            dims
        };

        let fr = cfg
            .as_ref()
            .and_then(|c| c.framerate)
            .or_else(|| (avg_fr.num > 0 && avg_fr.den > 0).then_some(Framerate(avg_fr)))
            .unwrap_or(Framerate::fps(25));

        let mut eb = VideoEncoder::builder()
            .codec(codec)
            .resolution(enc_w as u32, enc_h as u32)
            .pixel_format(enc_pix)
            .framerate(fr)
            .time_base(in_tb)
            .global_header(writer.wants_global_header());
        if let Some(c) = &cfg {
            if let Some(b) = c.bitrate {
                eb = eb.bitrate(b);
            }
            if let Some(p) = c.preset {
                eb = eb.preset(p);
            }
            if let Some(p) = c.profile {
                eb = eb.profile(p);
            }
        }
        let enc = eb.build()?;
        out_vidx = writer.add_stream_from_encoder(&enc)?;
        decoder = Some(dec);
        encoder = Some(enc);
    }

    // --- audio setup (stream copy) -----------------------------------------------------
    let mut out_aidx = None;
    let mut a_tb = Rational::new(1, 1);
    if let Some(aidx) = audio_idx {
        a_tb = reader.stream_time_base(aidx)?;
        out_aidx = Some(writer.add_stream_copy(&reader, aidx)?);
    }

    writer.write_header()?;

    // --- trim bounds -------------------------------------------------------------------
    let (trim_start, trim_end) = opts.trim.unwrap_or((0.0, f64::INFINITY));
    let v_start_ts = secs_to_ts(trim_start, v_tb);
    let a_start_ts = secs_to_ts(trim_start, a_tb);
    let v_tb_f64 = v_tb.as_f64();
    let a_tb_f64 = a_tb.as_f64();
    let in_trim = |secs: f64| secs >= trim_start && secs <= trim_end;

    // --- main loop ---------------------------------------------------------------------
    let started = Instant::now();
    let mut frames = 0u64;

    for packet in reader.packets() {
        let packet = packet?;
        let sidx = packet.stream_index();

        if Some(sidx) == video_idx {
            let dec = decoder.as_mut().unwrap();
            let enc = encoder.as_mut().unwrap();
            for frame in dec.decode(&packet)? {
                let frame = frame?;
                let secs = frame_secs(&frame, v_tb_f64);
                if !in_trim(secs) {
                    continue;
                }
                process_video_frame(frame, &mut vfilter, enc, &mut writer, out_vidx, v_start_ts, &mut frames)?;
                let elapsed = started.elapsed().as_secs_f64().max(1e-6);
                on_progress(Progress {
                    processed_secs: (secs - trim_start).max(0.0),
                    total_secs: total_secs - trim_start.min(total_secs),
                    frames,
                    fps: frames as f64 / elapsed,
                });
            }
        } else if Some(sidx) == audio_idx {
            // Stream copy: gate on trim, offset timestamps, remap to the output stream.
            let mut packet = packet;
            let secs = packet.pts() as f64 * a_tb_f64;
            if !in_trim(secs) {
                continue;
            }
            packet.offset_timestamps(a_start_ts);
            packet.set_stream_index(out_aidx.unwrap());
            writer.write_packet(&mut packet)?;
        }
    }

    // --- flush video: decoder → filter → encoder ---------------------------------------
    if let (Some(dec), Some(enc)) = (decoder.as_mut(), encoder.as_mut()) {
        let mut tail = Vec::new();
        for frame in dec.flush()? {
            tail.push(frame?);
        }
        for frame in tail {
            if !in_trim(frame_secs(&frame, v_tb_f64)) {
                continue;
            }
            process_video_frame(frame, &mut vfilter, enc, &mut writer, out_vidx, v_start_ts, &mut frames)?;
        }
        if let Some(vf) = vfilter.as_mut() {
            for frame in vf.flush()? {
                encode_and_mux(enc, &mut writer, out_vidx, frame, v_start_ts, &mut frames)?;
            }
        }
        // Flush the encoder itself.
        for pkt in enc.flush()? {
            let mut pkt = pkt?;
            pkt.set_stream_index(out_vidx);
            writer.write_packet(&mut pkt)?;
        }
    }

    writer.write_trailer()?;

    Ok(TranscodeSummary {
        frames,
        duration_secs: (total_secs - trim_start).max(0.0).min(total_secs),
    })
}
