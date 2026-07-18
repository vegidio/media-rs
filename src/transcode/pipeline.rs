//! The shared transcode loop: demux → decode → (filter) → encode → mux. Video and audio are
//! each either re-encoded (decode → filter → encode) or stream-copied. Tiers 1 and 2 both
//! drive this.

use super::config::{AudioConfig, VideoConfig};
use super::progress::{Progress, TranscodeSummary};
use crate::codec::decoder::Decoder;
use crate::codec::encoder::{AudioEncoder, VideoEncoder};
use crate::error::{Error, Result};
use crate::filter::{AudioFilter, AudioFilterChain, VideoFilter, VideoFilterChain};
use crate::format::{MediaReader, MediaWriter};
use crate::frame::Frame;
use crate::packet::Packet;
use crate::types::codec::{AudioCodec, VideoCodec};
use crate::types::rational::{Framerate, Rational};
use crate::types::stream_kind::StreamKind;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::thread;
use std::time::{Duration, Instant};

/// Depth of the producer→consumer work queue. Bounded so the producer can't race far ahead of
/// the encoder (backpressure + capped in-flight memory).
const CHANNEL_DEPTH: usize = 8;

/// Output frame rate used when neither the config nor the input stream advertises one.
const DEFAULT_FRAMERATE_FPS: u32 = 25;

/// The resolved set of options a transcode runs with.
pub(crate) struct TranscodeOptions {
    pub input: String,
    pub output: String,
    pub video: Option<VideoConfig>,
    pub audio: Option<AudioConfig>,
    pub drop_video: bool,
    pub drop_audio: bool,
    pub trim: Option<(f64, f64)>,
    pub video_filter: VideoFilterChain,
    pub audio_filter: AudioFilterChain,
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

/// The video codec the transcode will actually encode with: the user's explicit choice, else
/// the H.264 default. Resolved in one place so the pre-flight container guard and `setup_video`
/// can never disagree.
fn effective_video_codec(opts: &TranscodeOptions) -> VideoCodec {
    opts.video.as_ref().map(|c| c.codec).unwrap_or(VideoCodec::H264)
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
    let codec = effective_video_codec(opts);
    let (in_w, in_h) = (dec.width() as i32, dec.height() as i32);
    let in_pix = dec.pixel_format();
    let (tw, th) = cfg
        .as_ref()
        .and_then(|c| c.resolution)
        .map(|(w, h)| (w as i32, h as i32))
        .unwrap_or((in_w, in_h));

    // Compose the effective filter chain: user filters, plus an auto-scale when the
    // requested resolution differs from the input.
    let mut chain = opts.video_filter.clone();

    if (tw, th) != (in_w, in_h) {
        let mut stages = vec![format!("scale={tw}:{th}")];

        if !opts.video_filter.is_empty() {
            stages.push(opts.video_filter.description());
        }

        chain = VideoFilterChain::raw(stages.join(","));
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
        .unwrap_or(Framerate::fps(DEFAULT_FRAMERATE_FPS));

    let mut eb = VideoEncoder::builder()
        .codec(codec)
        .resolution(enc_w as u32, enc_h as u32)
        .pixel_format(enc_pix)
        .framerate(fr)
        .time_base(in_tb)
        .global_header(writer.wants_global_header());

    if let Some(c) = &cfg {
        eb = c.apply_to(eb);
    }

    let encoder = eb.build()?;
    let out_vidx = writer.add_stream_from_encoder(&encoder)?;

    Ok(VideoStage { decoder: dec, encoder, vfilter, out_vidx, v_tb: in_tb })
}

/// The configured audio half of a re-encoding transcode: decoder (producer side) + encoder and
/// optional filter (consumer side), the output stream index, and the source time base.
struct AudioStage {
    decoder: Decoder,
    encoder: AudioEncoder,
    afilter: Option<AudioFilter>,
    out_aidx: usize,
}

/// Decide which codec to *re-encode* audio to: the user's explicit choice, else the source
/// codec when the container accepts it (a filter-only re-encode), else the container's default.
fn resolve_audio_codec(
    opts: &TranscodeOptions,
    writer: &MediaWriter,
    src_codec_id: crate::sys::AVCodecID,
) -> Result<AudioCodec> {
    if let Some(c) = opts.audio.as_ref().map(|c| c.codec) {
        return Ok(c);
    }
    if writer.supports_codec(src_codec_id)
        && let Some(c) = AudioCodec::from_codec_id(src_codec_id)
    {
        return Ok(c);
    }
    AudioCodec::from_codec_id(writer.default_audio_codec_id()).ok_or(Error::InvalidConfig(
        "cannot encode audio for this container; set .audio(...) with a supported codec",
    ))
}

/// Build the decoder, encoder, and optional filter for audio stream `aidx`, and add the output
/// stream. Used whenever audio must be re-encoded (a config/filter was given, or the source
/// codec can't be stream-copied into the target container).
fn setup_audio(
    opts: &TranscodeOptions,
    reader: &mut MediaReader,
    writer: &mut MediaWriter,
    aidx: usize,
    codec: AudioCodec,
) -> Result<AudioStage> {
    let a_tb = reader.stream_time_base(aidx)?;
    let dec = reader.stream(aidx).decoder()?;

    let mut eb = AudioEncoder::builder()
        .codec(codec)
        .from_decoder(&dec)
        .global_header(writer.wants_global_header());
    if let Some(c) = &opts.audio {
        eb = c.apply_to(eb);
    }
    let encoder = eb.build()?;

    // The filter runs on the decoder's native format; its output frames feed the encoder, which
    // resamples them to the encode format.
    let afilter = if opts.audio_filter.is_empty() {
        None
    } else {
        Some(AudioFilter::new(
            dec.sample_rate() as i32,
            dec.sample_format(),
            dec.ch_layout_owned(),
            a_tb,
            &opts.audio_filter,
        )?)
    };

    let out_aidx = writer.add_stream_from_encoder(&encoder)?;
    Ok(AudioStage { decoder: dec, encoder, afilter, out_aidx })
}

/// One item handed from the demux/decode producer to the encode/mux consumer.
enum Work {
    /// A decoded, in-trim video frame plus its source-time position (seconds, for progress).
    Video { frame: Frame, secs: f64 },
    /// A decoded, in-trim audio frame to re-encode, plus its source-time position.
    AudioFrame { frame: Frame, secs: f64 },
    /// An already trim-gated, timestamp-offset, remapped audio packet to mux verbatim (copy).
    Audio(Packet),
}

/// Stamp each encoded audio packet with its output stream index and mux it.
fn mux_audio_packets(writer: &mut MediaWriter, packets: Vec<Packet>, out_aidx: usize) -> Result<()> {
    for mut pkt in packets {
        pkt.set_stream_index(out_aidx);
        writer.write_packet(&mut pkt)?;
    }
    Ok(())
}

/// Filter (if configured) and encode one audio frame, muxing the resulting packets to
/// `out_aidx`. The encoder assigns continuous, zero-based timestamps, so no trim offset is
/// needed (trim-gated frames simply start the timeline at zero).
fn encode_audio_frame(
    frame: Frame,
    afilter: &mut Option<AudioFilter>,
    encoder: &mut AudioEncoder,
    writer: &mut MediaWriter,
    out_aidx: usize,
) -> Result<()> {
    match afilter {
        Some(af) => {
            for f in af.filter(frame)? {
                mux_audio_packets(writer, encoder.encode(&f)?, out_aidx)?;
            }
        }
        None => mux_audio_packets(writer, encoder.encode(&frame)?, out_aidx)?,
    }
    Ok(())
}

/// The consumer-side (encode/mux) half of the video stage: everything the consumer thread keeps
/// after the decoder is handed to the producer. Bundling `encoder`+`out_vidx` here is what lets
/// the consumer loop use `vc.out_vidx` directly instead of an `out_vidx.unwrap()`.
struct VideoConsumer {
    encoder: VideoEncoder,
    vfilter: Option<VideoFilter>,
    out_vidx: usize,
}

/// The consumer-side half of the audio stage (re-encode path only).
struct AudioConsumer {
    encoder: AudioEncoder,
    afilter: Option<AudioFilter>,
    out_aidx: usize,
}

/// Timing/offset context the consumer needs for pts adjustment and progress reporting.
struct ConsumerCtx {
    /// The video trim-start timestamp (source time base) subtracted from each frame's pts.
    v_start_ts: i64,
    /// The trim start in seconds, used to offset the progress position to zero.
    trim_start: f64,
    /// The output's real (trimmed) duration in seconds — the progress denominator.
    span: f64,
    /// When encoding began, for throughput.
    started: Instant,
}

/// The encode/mux half of the pipeline: pull [`Work`] off the channel, filter+encode video and
/// audio (or mux copied audio), then at end of stream flush the filters and encoders. Returns
/// the encoded video-frame count for the summary. Runs on the caller thread so `on_progress`
/// (a `FnMut`) never has to cross a thread boundary.
fn run_consumer(
    rx: Receiver<Work>,
    mut video: Option<VideoConsumer>,
    mut audio: Option<AudioConsumer>,
    writer: &mut MediaWriter,
    ctx: ConsumerCtx,
    mut on_progress: impl FnMut(Progress),
) -> Result<u64> {
    let ConsumerCtx { v_start_ts, trim_start, span, started } = ctx;
    let mut frames = 0u64;
    // With no video stream, audio drives progress instead.
    let audio_drives_progress = video.is_none();

    for work in rx {
        match work {
            Work::Video { frame, secs } => {
                // A video frame can only arrive when the video stage was configured.
                let vc = video.as_mut().ok_or(Error::Bug("video frame with no encoder configured"))?;
                process_video_frame(
                    frame,
                    &mut vc.vfilter,
                    &mut vc.encoder,
                    writer,
                    vc.out_vidx,
                    v_start_ts,
                    &mut frames,
                )?;
                on_progress(Progress::new((secs - trim_start).max(0.0), span, frames, started));
            }
            Work::AudioFrame { frame, secs } => {
                let ac = audio.as_mut().ok_or(Error::Bug("audio frame with no encoder configured"))?;
                encode_audio_frame(frame, &mut ac.afilter, &mut ac.encoder, writer, ac.out_aidx)?;
                if audio_drives_progress {
                    on_progress(Progress::new((secs - trim_start).max(0.0), span, frames, started));
                }
            }
            Work::Audio(mut pkt) => writer.write_packet(&mut pkt)?,
        }
    }

    // End of stream: flush the video filter+encoder…
    if let Some(vc) = video.as_mut() {
        if let Some(vf) = vc.vfilter.as_mut() {
            for frame in vf.flush()? {
                encode_and_mux(&mut vc.encoder, writer, vc.out_vidx, frame, v_start_ts, &mut frames)?;
            }
        }

        for pkt in vc.encoder.flush()? {
            let mut pkt = pkt?;
            pkt.set_stream_index(vc.out_vidx);
            writer.write_packet(&mut pkt)?;
        }
    }

    // …then the audio filter+encoder.
    if let Some(ac) = audio.as_mut() {
        if let Some(af) = ac.afilter.as_mut() {
            for frame in af.flush()? {
                mux_audio_packets(writer, ac.encoder.encode(&frame)?, ac.out_aidx)?;
            }
        }

        mux_audio_packets(writer, ac.encoder.flush()?, ac.out_aidx)?;
    }

    Ok(frames)
}

/// Everything the producer thread owns: the reader, the two decoders, and the trim/remap state
/// it needs to gate and forward work. Built in [`run`] and moved onto the spawned thread.
struct ProducerCtx {
    reader: MediaReader,
    video_decoder: Option<Decoder>,
    audio_decoder: Option<Decoder>,
    video_idx: Option<usize>,
    audio_idx: Option<usize>,
    /// Output stream index for stream-copied audio (the copy path stamps packets with it).
    out_aidx: Option<usize>,
    v_tb: Rational,
    a_tb: Rational,
    a_tb_f64: f64,
    /// Trim-start timestamp (audio time base) added to copied audio packets.
    a_start_ts: i64,
    trim_start: f64,
    trim_end: f64,
}

/// The demux/decode half of the pipeline (runs on the spawned producer thread): demux packets,
/// decode video and audio, gate everything on the trim window, and forward [`Work`] to the
/// consumer over `tx`. Audio is either decoded (re-encode path) or forwarded verbatim (copy path).
fn producer(ctx: ProducerCtx, tx: SyncSender<Work>) -> Result<()> {
    let ProducerCtx {
        mut reader,
        mut video_decoder,
        mut audio_decoder,
        video_idx,
        audio_idx,
        out_aidx,
        v_tb,
        a_tb,
        a_tb_f64,
        a_start_ts,
        trim_start,
        trim_end,
    } = ctx;
    let in_trim = |secs: f64| secs >= trim_start && secs <= trim_end;

    // Fast trim: instead of decoding and discarding everything before `trim_start`, seek to the keyframe at/just
    // before it and reset the decoders. `in_trim` still drops the frames between that keyframe and `trim_start`, so
    // the emitted frames are unchanged — we just skip decoding the prefix. Seek failure (e.g. a
    // non-seekable input) is non-fatal: we fall back to the linear scan, exactly the previous behaviour.
    if trim_start > 0.0 {
        let seek_idx = video_idx.or(audio_idx).unwrap();
        if reader.seek(seek_idx, Duration::from_secs_f64(trim_start)).is_ok() {
            if let Some(dec) = video_decoder.as_mut() {
                dec.reset();
            }
            if let Some(dec) = audio_decoder.as_mut() {
                dec.reset();
            }
        }
    }

    for packet in reader.packets() {
        let packet = packet?;
        let sidx = packet.stream_index();

        if Some(sidx) == video_idx {
            let dec = video_decoder.as_mut().unwrap();

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
            if let Some(dec) = audio_decoder.as_mut() {
                // Re-encode: decode, gate on trim, hand decoded frames to the consumer.
                for frame in dec.decode(&packet)? {
                    let frame = frame?;
                    let secs = frame_secs(&frame, a_tb);
                    if !in_trim(secs) {
                        continue;
                    }
                    if tx.send(Work::AudioFrame { frame, secs }).is_err() {
                        return Ok(());
                    }
                }
            } else {
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
    }

    // Flush a decoder's buffered tail and forward its in-trim frames. Collect first so the decoder's mutable borrow
    // ends before we send. Returns `false` if the consumer has hung up (channel closed), so the caller can stop.
    let flush_tail = |dec: &mut Decoder, tb: Rational, wrap: fn(Frame, f64) -> Work| -> Result<bool> {
        let mut tail = Vec::new();
        for frame in dec.flush()? {
            tail.push(frame?);
        }

        for frame in tail {
            let secs = frame_secs(&frame, tb);
            if !in_trim(secs) {
                continue;
            }

            if tx.send(wrap(frame, secs)).is_err() {
                return Ok(false);
            }
        }

        Ok(true)
    };

    if let Some(dec) = video_decoder.as_mut()
        && !flush_tail(dec, v_tb, |frame, secs| Work::Video { frame, secs })?
    {
        return Ok(());
    }

    // Audio decoder tail (re-encode path only).
    if let Some(dec) = audio_decoder.as_mut()
        && !flush_tail(dec, a_tb, |frame, secs| Work::AudioFrame { frame, secs })?
    {
        return Ok(());
    }

    Ok(())
    // tx dropped here → the consumer's loop ends once the channel drains.
}

pub(crate) fn run(opts: &TranscodeOptions, on_progress: impl FnMut(Progress)) -> Result<TranscodeSummary> {
    let mut reader = MediaReader::open(&opts.input)?;
    let total_secs = reader.duration_secs();

    let video_idx = if opts.drop_video { None } else { reader.best_stream(StreamKind::Video).ok() };

    let audio_idx = if opts.drop_audio { None } else { reader.best_stream(StreamKind::Audio).ok() };

    if video_idx.is_none() && audio_idx.is_none() {
        return Err(Error::InvalidConfig("nothing to transcode (both video and audio dropped or absent)"));
    }

    let mut writer = MediaWriter::create(&opts.output)?;

    // Fail loudly rather than letting the muxer reject a video stream deep down: if the input
    // has video (not dropped) but the container can't hold the codec we'd encode it as, stop
    // now with actionable advice. (The MP3 muxer, for one, allows only cover-art images, not a
    // real video codec.)
    if video_idx.is_some() {
        let video_codec = effective_video_codec(opts);
        if !writer.supports_codec(video_codec.codec_id()) {
            return Err(Error::InvalidConfig(
                "output container does not support the video stream; call .drop_video() to drop it (or use a container/codec that supports video)",
            ));
        }
    }

    // --- video setup -------------------------------------------------------------------
    let video = match video_idx {
        Some(vidx) => Some(setup_video(opts, &mut reader, &mut writer, vidx)?),
        None => None,
    };

    // --- audio setup: re-encode when a config/filter was given or the source codec can't be
    // copied into this container; otherwise stream-copy verbatim (fast, lossless). ----------
    let mut out_aidx = None;
    let mut a_tb = Rational::ONE;
    let mut audio_stage: Option<AudioStage> = None;
    if let Some(aidx) = audio_idx {
        a_tb = reader.stream_time_base(aidx)?;
        let src_codec_id = reader.input().stream_codec_id(aidx)?;
        let want_reencode = opts.audio.is_some() || !opts.audio_filter.is_empty();
        let src_supported = writer.supports_codec(src_codec_id);

        if want_reencode || !src_supported {
            let codec = resolve_audio_codec(opts, &writer, src_codec_id)?;
            let stage = setup_audio(opts, &mut reader, &mut writer, aidx, codec)?;
            out_aidx = Some(stage.out_aidx);
            audio_stage = Some(stage);
        } else {
            out_aidx = Some(writer.add_stream_copy(&reader, aidx)?);
        }
    }

    writer.write_header()?;

    // --- trim bounds -------------------------------------------------------------------
    let v_tb = video.as_ref().map(|s| s.v_tb).unwrap_or(Rational::ONE);
    let (trim_start, trim_end) = opts.trim.unwrap_or((0.0, f64::INFINITY));
    let v_start_ts = v_tb.ts_from_secs(trim_start);
    let a_start_ts = a_tb.ts_from_secs(trim_start);
    let a_tb_f64 = a_tb.as_f64();

    // The output's real duration: the trimmed span clamped to the file. Drives both the progress
    // denominator and the summary, so a bounded trim (e.g. 3s..=6s) reports ~3s, not the whole file.
    let out_span = (trim_end.min(total_secs) - trim_start).max(0.0);

    let started = Instant::now();

    // Split each stage across the two pipeline halves: the decoder demuxes/decodes on the producer
    // thread; the encoder+filter mux on the consumer (this) thread with the writer.
    let (video_decoder, video_consumer) = match video {
        Some(s) => (
            Some(s.decoder),
            Some(VideoConsumer { encoder: s.encoder, vfilter: s.vfilter, out_vidx: s.out_vidx }),
        ),
        None => (None, None),
    };
    let (audio_decoder, audio_consumer) = match audio_stage {
        Some(s) => (
            Some(s.decoder),
            Some(AudioConsumer { encoder: s.encoder, afilter: s.afilter, out_aidx: s.out_aidx }),
        ),
        None => (None, None),
    };

    let (tx, rx) = sync_channel::<Work>(CHANNEL_DEPTH);

    // --- producer: demux + decode (video and audio) on its own thread ------------------
    let producer_ctx = ProducerCtx {
        reader,
        video_decoder,
        audio_decoder,
        video_idx,
        audio_idx,
        out_aidx,
        v_tb,
        a_tb,
        a_tb_f64,
        a_start_ts,
        trim_start,
        trim_end,
    };
    let producer_handle = thread::spawn(move || producer(producer_ctx, tx));

    // --- consumer: filter + encode + mux (this thread) ---------------------------------
    let consumer_ctx = ConsumerCtx { v_start_ts, trim_start, span: out_span, started };
    let consumer_result = run_consumer(rx, video_consumer, audio_consumer, &mut writer, consumer_ctx, on_progress);

    // Always join the producer before propagating, so no worker thread is left detached.
    let producer_result = producer_handle.join();

    let frames = consumer_result?;
    match producer_result {
        Ok(inner) => inner?,
        Err(_) => return Err(Error::ThreadPanicked),
    }

    writer.write_trailer()?;

    Ok(TranscodeSummary { frames, duration_secs: out_span })
}
