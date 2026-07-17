//! The shared transcode loop: demux → decode → (filter) → encode → mux, with audio
//! stream-copied. Tiers 1 and 2 both drive this.

use super::config::VideoConfig;
use super::progress::{Progress, TranscodeSummary};
use crate::codec::decoder::Decoder;
use crate::codec::encoder::VideoEncoder;
use crate::error::{Error, Result};
use crate::filter::{FilterChain, VideoFilter};
use crate::format::{MediaReader, MediaWriter};
use crate::frame::Frame;
use crate::packet::Packet;
use crate::types::codec::VideoCodec;
use crate::types::rational::{Framerate, Rational};
use crate::types::stream_kind::StreamKind;
use std::sync::mpsc::{Receiver, sync_channel};
use std::thread;
use std::time::{Duration, Instant};

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

/// Encode one (already filtered) frame and mux the resulting packets.
fn encode_and_mux(
    encoder: &mut VideoEncoder,
    writer: &mut MediaWriter,
    out_idx: usize,
    mut frame: Frame,
    trim_start_ts: i64,
    frames: &mut u64,
) -> Result<()> {
    let ts = frame.best_ts();
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
fn frame_secs(frame: &Frame, tb: Rational) -> f64 {
    tb.secs_from_ts(frame.best_ts())
}

/// The configured video half of a transcode: the decoder/encoder pair, the optional filter
/// graph, the output stream index, and the source (video) time base.
struct VideoStage {
    decoder: Decoder,
    encoder: VideoEncoder,
    vfilter: Option<VideoFilter>,
    out_vidx: usize,
    v_tb: Rational,
}

/// Build the decoder, effective filter graph, and encoder for video stream `vidx`, and add
/// the output stream. The filter graph is built eagerly so the encoder is sized to the graph's
/// *actual* output — the only correct dimension/format even for arbitrary user filters.
fn setup_video(
    opts: &TranscodeOptions,
    reader: &mut MediaReader,
    writer: &mut MediaWriter,
    vidx: usize,
) -> Result<VideoStage> {
    let in_tb = reader.stream_time_base(vidx)?;
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

    // Compose the effective filter chain: user filters, plus an auto-scale when the
    // requested resolution differs from the input.
    let mut chain = opts.filter.clone();

    if (tw, th) != (in_w, in_h) {
        let mut stages = vec![format!("scale={tw}:{th}")];

        if !opts.filter.is_empty() {
            stages.push(opts.filter.description());
        }

        chain = FilterChain::raw(stages.join(","));
    }

    let mut vfilter = None;

    let (enc_w, enc_h, enc_pix) = if chain.is_empty() {
        (in_w, in_h, in_pix)
    } else {
        let f = VideoFilter::new(in_w, in_h, in_pix, in_tb, Rational::ONE, &chain)?;
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

    let encoder = eb.build()?;
    let out_vidx = writer.add_stream_from_encoder(&encoder)?;

    Ok(VideoStage {
        decoder: dec,
        encoder,
        vfilter,
        out_vidx,
        v_tb: in_tb,
    })
}

/// One item handed from the demux/decode producer to the encode/mux consumer.
enum Work {
    /// A decoded, in-trim video frame plus its source-time position (seconds, for progress).
    Video { frame: Frame, secs: f64 },
    /// An already trim-gated, timestamp-offset, remapped audio packet to mux verbatim.
    Audio(Packet),
}

/// The encode/mux half of the pipeline: pull [`Work`] off the channel, filter+encode video and
/// mux audio, then at end of stream flush the filter and encoder. Returns the encoded frame
/// count for the summary. Runs on the caller thread so `on_progress` (a `FnMut`) never has to
/// cross a thread boundary.
#[allow(clippy::too_many_arguments)]
fn run_consumer(
    rx: Receiver<Work>,
    mut encoder: Option<VideoEncoder>,
    mut vfilter: Option<VideoFilter>,
    writer: &mut MediaWriter,
    out_vidx: Option<usize>,
    v_start_ts: i64,
    trim_start: f64,
    total_secs: f64,
    started: Instant,
    mut on_progress: impl FnMut(Progress),
) -> Result<u64> {
    let mut frames = 0u64;
    let span = total_secs - trim_start.min(total_secs);

    for work in rx {
        match work {
            Work::Video { frame, secs } => {
                // A video frame can only arrive when the video stage (encoder) was configured.
                let enc = encoder
                    .as_mut()
                    .ok_or(Error::InvalidConfig("video frame with no encoder configured"))?;
                let ov = out_vidx.unwrap();
                process_video_frame(frame, &mut vfilter, enc, writer, ov, v_start_ts, &mut frames)?;
                on_progress(Progress::new((secs - trim_start).max(0.0), span, frames, started));
            }
            Work::Audio(mut pkt) => writer.write_packet(&mut pkt)?,
        }
    }

    // End of stream: flush the filter, then the encoder (video only).
    if let Some(enc) = encoder.as_mut() {
        let ov = out_vidx.unwrap();

        if let Some(vf) = vfilter.as_mut() {
            for frame in vf.flush()? {
                encode_and_mux(enc, writer, ov, frame, v_start_ts, &mut frames)?;
            }
        }

        for pkt in enc.flush()? {
            let mut pkt = pkt?;
            pkt.set_stream_index(ov);
            writer.write_packet(&mut pkt)?;
        }
    }

    Ok(frames)
}

pub(crate) fn run(opts: &TranscodeOptions, on_progress: impl FnMut(Progress)) -> Result<TranscodeSummary> {
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
    let video = match video_idx {
        Some(vidx) => Some(setup_video(opts, &mut reader, &mut writer, vidx)?),
        None => None,
    };

    // --- audio setup (stream copy) -----------------------------------------------------
    let mut out_aidx = None;
    let mut a_tb = Rational::ONE;
    if let Some(aidx) = audio_idx {
        a_tb = reader.stream_time_base(aidx)?;
        out_aidx = Some(writer.add_stream_copy(&reader, aidx)?);
    }

    writer.write_header()?;

    // --- trim bounds -------------------------------------------------------------------
    let v_tb = video.as_ref().map(|s| s.v_tb).unwrap_or(Rational::ONE);
    let (trim_start, trim_end) = opts.trim.unwrap_or((0.0, f64::INFINITY));
    let v_start_ts = v_tb.ts_from_secs(trim_start);
    let a_start_ts = a_tb.ts_from_secs(trim_start);
    let a_tb_f64 = a_tb.as_f64();

    let started = Instant::now();

    // Split the video stage across the two pipeline halves: the decoder demuxes/decodes on the
    // producer thread; the filter+encoder mux on the consumer (this) thread with the writer.
    let (decoder, encoder, vfilter, out_vidx) = match video {
        Some(s) => (Some(s.decoder), Some(s.encoder), s.vfilter, Some(s.out_vidx)),
        None => (None, None, None, None),
    };

    // Bounded so the producer can't race far ahead of the encoder (backpressure + capped memory).
    let (tx, rx) = sync_channel::<Work>(8);

    // --- producer: demux + decode (+ audio pass-through) -------------------------------
    let producer = thread::spawn(move || -> Result<()> {
        let mut reader = reader;
        let mut decoder = decoder;
        let in_trim = |secs: f64| secs >= trim_start && secs <= trim_end;

        // Fast trim: instead of decoding and discarding everything before `trim_start`, seek to the keyframe at/just
        // before it and reset the decoder. `in_trim` still drops the frames between that keyframe and `trim_start`, so
        // the emitted frames are unchanged — we just skip decoding the prefix. Seek failure (e.g. a non-seekable input)
        // is non-fatal: we fall back to the linear scan, which is exactly the previous behaviour.
        if trim_start > 0.0 {
            let seek_idx = video_idx.or(audio_idx).unwrap();
            if reader.seek(seek_idx, Duration::from_secs_f64(trim_start)).is_ok()
                && let Some(dec) = decoder.as_mut()
            {
                dec.reset();
            }
        }

        for packet in reader.packets() {
            let packet = packet?;
            let sidx = packet.stream_index();

            if Some(sidx) == video_idx {
                let dec = decoder.as_mut().unwrap();

                for frame in dec.decode(&packet)? {
                    let frame = frame?;
                    let secs = frame_secs(&frame, v_tb);

                    if !in_trim(secs) {
                        continue;
                    }

                    // A closed channel means the consumer has stopped (errored); we're done.
                    if tx.send(Work::Video { frame, secs }).is_err() {
                        return Ok(());
                    }
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

                if tx.send(Work::Audio(packet)).is_err() {
                    return Ok(());
                }
            }
        }

        // Flush the decoder tail; collect first so its mutable borrow ends before we send.
        if let Some(dec) = decoder.as_mut() {
            let mut tail = Vec::new();

            for frame in dec.flush()? {
                tail.push(frame?);
            }

            for frame in tail {
                let secs = frame_secs(&frame, v_tb);
                if !in_trim(secs) {
                    continue;
                }
                if tx.send(Work::Video { frame, secs }).is_err() {
                    return Ok(());
                }
            }
        }

        Ok(())
        // tx dropped here → the consumer's loop ends once the channel drains.
    });

    // --- consumer: filter + encode + mux (this thread) ---------------------------------
    let consumer_result = run_consumer(
        rx, encoder, vfilter, &mut writer, out_vidx, v_start_ts, trim_start, total_secs, started,
        on_progress,
    );

    // Always join the producer before propagating, so no worker thread is left detached.
    let producer_result = producer.join();

    let frames = consumer_result?;
    match producer_result {
        Ok(inner) => inner?,
        Err(_) => {
            return Err(Error::Internal {
                code: 0,
                message: "transcode producer thread panicked".to_owned(),
            });
        }
    }

    writer.write_trailer()?;

    Ok(TranscodeSummary {
        frames,
        duration_secs: (total_secs - trim_start).max(0.0).min(total_secs),
    })
}
