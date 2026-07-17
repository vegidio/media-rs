//! Encoding: turn decoded [`Frame`]s back into compressed [`Packet`]s (video and audio).

use crate::error::{Error, Result};
use crate::frame::Frame;
use crate::packet::Packet;
use crate::raw::audio_fifo::AudioFifo;
use crate::raw::codec_context::{CodecContext, drain_received, find_encoder_by_name};
use crate::raw::frame::RawFrame;
use crate::raw::packet::RawPacket;
use crate::raw::resampler::ResampleContext;
use crate::sys;
use crate::types::audio::SampleRate;
use crate::types::channel_layout::{ChannelLayout, Channels};
use crate::types::codec::{AudioCodec, VideoCodec};
use crate::types::pixel_format::PixelFormat;
use crate::types::preset::H264Preset;
use crate::types::profile::H264Profile;
use crate::types::rational::{Bitrate, Framerate, Rational};
use crate::types::sample_format::SampleFormat;

/// Types that can back a muxer output stream — the video and audio encoders. Sealed: not
/// implementable outside this crate. This is what lets
/// [`MediaWriter::add_stream_from_encoder`](crate::format::MediaWriter::add_stream_from_encoder)
/// accept either encoder under one method name.
#[allow(private_bounds)]
pub trait Encoder: sealed::Encode {}

pub(crate) mod sealed {
    use crate::raw::codec_context::CodecContext;
    use crate::types::rational::Rational;

    /// The muxer-facing view of an encoder: its opened codec context (for stream params) and
    /// the time base its packets carry.
    pub(crate) trait Encode {
        fn codec_ctx(&self) -> &CodecContext;
        fn time_base(&self) -> Rational;
    }
}

/// A configured, opened video encoder.
///
/// Build one with [`VideoEncoder::builder`]. Feed decoded frames with
/// [`encode`](Self::encode) and drain the returned iterator; at end of stream call
/// [`flush`](Self::flush) to emit any buffered packets.
pub struct VideoEncoder {
    ctx: CodecContext,
    recv: RawPacket,
    time_base: Rational,
}

impl VideoEncoder {
    /// Start configuring a video encoder.
    pub fn builder() -> VideoEncoderBuilder {
        VideoEncoderBuilder::default()
    }

    /// Submit a frame and return an iterator over the packets it produces.
    ///
    /// The frame's `pts` must already be expressed in this encoder's
    /// [`time_base`](Self::time_base).
    pub fn encode(&mut self, frame: &Frame) -> Result<EncodeIter<'_>> {
        self.ctx.send_frame(Some(&frame.raw))?;
        Ok(EncodeIter { enc: self })
    }

    /// Flush the encoder at end of stream and return any buffered packets.
    ///
    /// Failing to drain this iterator truncates the output — the encoder may be holding the
    /// final GOP.
    #[must_use = "the flush iterator yields the encoder's trailing packets; drain them or the output is truncated"]
    pub fn flush(&mut self) -> Result<EncodeIter<'_>> {
        self.ctx.send_frame(None)?;
        Ok(EncodeIter { enc: self })
    }

    /// The encoder's time base. Packets it produces carry timestamps in this base.
    pub fn time_base(&self) -> Rational {
        self.time_base
    }
}

impl sealed::Encode for VideoEncoder {
    fn codec_ctx(&self) -> &CodecContext {
        &self.ctx
    }
    fn time_base(&self) -> Rational {
        self.time_base
    }
}
impl Encoder for VideoEncoder {}

/// Iterator over the packets produced by one [`VideoEncoder::encode`]/[`flush`] call.
///
/// [`flush`]: VideoEncoder::flush
#[must_use = "encoding is lazy; iterate to actually receive packets"]
pub struct EncodeIter<'e> {
    enc: &'e mut VideoEncoder,
}

impl Iterator for EncodeIter<'_> {
    type Item = Result<Packet>;

    fn next(&mut self) -> Option<Self::Item> {
        let received = self.enc.ctx.receive_packet(&mut self.enc.recv);
        drain_received(received, || Ok(Packet::from_raw(self.enc.recv.move_out()?)))
    }
}

/// Builder for a [`VideoEncoder`].
pub struct VideoEncoderBuilder {
    codec: Option<VideoCodec>,
    width: Option<u32>,
    height: Option<u32>,
    pix_fmt: Option<PixelFormat>,
    framerate: Option<Framerate>,
    time_base: Option<Rational>,
    bitrate: Option<Bitrate>,
    preset: Option<H264Preset>,
    profile: Option<H264Profile>,
    gop_size: Option<i32>,
    global_header: bool,
}

impl Default for VideoEncoderBuilder {
    fn default() -> Self {
        Self {
            codec: None,
            width: None,
            height: None,
            pix_fmt: None,
            framerate: None,
            time_base: None,
            bitrate: None,
            preset: None,
            profile: None,
            gop_size: None,
            // MP4/MKV/WebM need codec extradata in the container header; defaulting this on
            // makes the common case correct without the caller knowing the container.
            global_header: true,
        }
    }
}

impl VideoEncoderBuilder {
    /// The codec to encode with (required).
    pub fn codec(mut self, codec: VideoCodec) -> Self {
        self.codec = Some(codec);
        self
    }

    /// Output resolution in pixels. Kept as `u32` until `build()`, which converts to the
    /// encoder's `i32` and rejects out-of-range values as [`Error::UnsupportedResolution`].
    pub fn resolution(mut self, width: u32, height: u32) -> Self {
        self.width = Some(width);
        self.height = Some(height);
        self
    }

    /// Inherit resolution, pixel format and frame rate from a decoder. The most common way
    /// to set up a transcode that keeps the input's geometry.
    pub fn from_decoder(mut self, decoder: &crate::codec::decoder::Decoder) -> Self {
        let ctx = decoder.codec_ctx();
        self.width.get_or_insert(ctx.width().max(0) as u32);
        self.height.get_or_insert(ctx.height().max(0) as u32);
        self.pix_fmt.get_or_insert(PixelFormat::from_av(ctx.pix_fmt()));
        let fr = ctx.framerate();
        if self.framerate.is_none() && fr.num > 0 && fr.den > 0 {
            self.framerate = Some(Framerate(fr));
        }
        self
    }

    /// The pixel format to encode in (defaults to the decoder's, else YUV420p).
    pub fn pixel_format(mut self, pix_fmt: PixelFormat) -> Self {
        self.pix_fmt = Some(pix_fmt);
        self
    }

    /// The output frame rate.
    pub fn framerate(mut self, framerate: Framerate) -> Self {
        self.framerate = Some(framerate);
        self
    }

    /// The encoder time base for incoming frame timestamps. Defaults to the input stream's
    /// time base when set via the pipeline, else `1/framerate`.
    pub fn time_base(mut self, time_base: Rational) -> Self {
        self.time_base = Some(time_base);
        self
    }

    /// Target bit rate.
    pub fn bitrate(mut self, bitrate: Bitrate) -> Self {
        self.bitrate = Some(bitrate);
        self
    }

    /// Speed/quality preset (applied for H.264/H.265).
    pub fn preset(mut self, preset: H264Preset) -> Self {
        self.preset = Some(preset);
        self
    }

    /// Codec profile (applied for H.264/H.265).
    pub fn profile(mut self, profile: H264Profile) -> Self {
        self.profile = Some(profile);
        self
    }

    /// Keyframe interval (group-of-pictures size).
    pub fn gop_size(mut self, gop_size: u32) -> Self {
        // Saturate so an absurd value can't wrap to a negative GOP size.
        self.gop_size = Some(i32::try_from(gop_size).unwrap_or(i32::MAX));
        self
    }

    /// Override the global-header flag (on by default; needed for MP4/MKV/WebM).
    pub fn global_header(mut self, enabled: bool) -> Self {
        self.global_header = enabled;
        self
    }

    /// Validate the configuration, open the encoder, and return it.
    pub fn build(self) -> Result<VideoEncoder> {
        crate::log::ensure_init();
        let codec = self.codec.ok_or(Error::InvalidConfig("video encoder requires a codec"))?;
        let width = self.width.ok_or(Error::InvalidConfig("video encoder requires a resolution"))?;
        let height = self.height.ok_or(Error::InvalidConfig("video encoder requires a resolution"))?;
        // The encoder wants `i32`; a `0` or an out-of-`i32`-range dimension is not encodable.
        // Report the requested values (saturated for display) rather than a wrapped number.
        let (Ok(width), Ok(height)) = (i32::try_from(width), i32::try_from(height)) else {
            return Err(Error::UnsupportedResolution {
                width: width.min(i32::MAX as u32) as i32,
                height: height.min(i32::MAX as u32) as i32,
            });
        };
        if width <= 0 || height <= 0 {
            return Err(Error::UnsupportedResolution { width, height });
        }

        let av_codec = find_encoder_by_name(codec.encoder_name())?;
        let mut ctx = CodecContext::alloc(av_codec)?;

        let pix_fmt = self.pix_fmt.unwrap_or(PixelFormat::Yuv420p);
        ctx.set_video_format(width, height, pix_fmt.to_av());

        let framerate = self.framerate.unwrap_or(Framerate::fps(25));
        let time_base = self.time_base.unwrap_or_else(|| framerate.time_base());
        ctx.set_time_base(time_base);
        ctx.set_framerate(framerate.0);

        if let Some(b) = self.bitrate {
            ctx.set_bit_rate(b.as_bps());
        }
        ctx.set_gop_size(self.gop_size.unwrap_or(12));
        if self.global_header {
            ctx.set_global_header();
        }

        // Encode multithreaded, mirroring the decoder. `0` sizes the pool to the CPU (for
        // libx264/x265 it hands off to x264's own auto-threading); passing FRAME|SLICE — rather
        // than SLICE alone — keeps x264 on frame threads, which its wrapper selects unless
        // thread_type == FF_THREAD_SLICE exactly. Without this the encoder defaults to a single
        // thread.
        ctx.set_threading(0, (sys::FF_THREAD_FRAME | sys::FF_THREAD_SLICE) as i32);

        // preset/profile are x264/x265 private options.
        if matches!(codec, VideoCodec::H264 | VideoCodec::H265) {
            if let Some(p) = self.preset {
                ctx.set_opt("preset", p.as_str())?;
            }
            if let Some(p) = self.profile {
                ctx.set_opt("profile", p.as_str())?;
            }
        }

        ctx.open()?;
        Ok(VideoEncoder { ctx, recv: RawPacket::alloc()?, time_base })
    }
}

/// A configured, opened audio encoder.
///
/// Unlike video, audio encoders demand a fixed number of samples per frame, and their input
/// must be in a specific sample format / rate / layout. [`AudioEncoder`] hides both: it
/// resamples every incoming [`Frame`] to the encoder's format and buffers samples in an
/// internal FIFO, emitting exactly-sized frames. Because one input frame may map to zero or
/// several output packets, [`encode`](Self::encode) returns a `Vec<Packet>` rather than the
/// lazy iterator [`VideoEncoder`] uses.
pub struct AudioEncoder {
    ctx: CodecContext,
    recv: RawPacket,
    time_base: Rational,
    /// Encoder input format (what we resample *to*).
    out_fmt: sys::AVSampleFormat,
    out_rate: i32,
    out_layout: ChannelLayout,
    /// Built lazily from the first frame's actual format (the authoritative decoder output).
    resampler: Option<ResampleContext>,
    fifo: AudioFifo,
    /// Samples per encoded frame; a fixed chunk for variable-frame-size codecs.
    frame_size: i32,
    /// Running output-sample counter, used as each encoder frame's pts (time base `1/rate`).
    next_pts: i64,
}

impl AudioEncoder {
    /// Start configuring an audio encoder.
    pub fn builder() -> AudioEncoderBuilder {
        AudioEncoderBuilder::default()
    }

    /// Submit a decoded frame; returns any packets that became ready. The frame is resampled to
    /// the encoder's format automatically, so its input format need not match.
    pub fn encode(&mut self, frame: &Frame) -> Result<Vec<Packet>> {
        self.ensure_resampler(&frame.raw)?;
        self.push_resampled(Some(&frame.raw))?;
        let mut out = Vec::new();
        self.drain_fifo(&mut out, false)?;
        Ok(out)
    }

    /// Flush at end of stream: drain the resampler tail, emit the final (possibly partial)
    /// frame, then drain the encoder. Returns any remaining packets.
    #[must_use = "the flush result carries the encoder's trailing packets; write them or the output is truncated"]
    pub fn flush(&mut self) -> Result<Vec<Packet>> {
        let mut out = Vec::new();
        if self.resampler.is_some() {
            self.push_resampled(None)?;
        }
        self.drain_fifo(&mut out, true)?;
        self.ctx.send_frame(None)?;
        self.recv_packets(&mut out)?;
        Ok(out)
    }

    /// The encoder's time base (`1/sample_rate`). Packets it produces carry timestamps here.
    pub fn time_base(&self) -> Rational {
        self.time_base
    }

    /// Build the resampler on first use, from the frame's real input format to the encoder's.
    fn ensure_resampler(&mut self, frame: &RawFrame) -> Result<()> {
        if self.resampler.is_some() {
            return Ok(());
        }
        let in_fmt = frame.format();
        let in_rate = frame.sample_rate().max(self.out_rate);
        let in_layout = frame.ch_layout_copy();
        self.resampler = Some(ResampleContext::new(
            in_fmt,
            in_rate,
            &in_layout,
            self.out_fmt,
            self.out_rate,
            &self.out_layout,
        )?);
        Ok(())
    }

    /// Resample `frame` (or flush the resampler when `None`) and append the result to the FIFO.
    fn push_resampled(&mut self, frame: Option<&RawFrame>) -> Result<()> {
        let resampler = self.resampler.as_mut().ok_or(Error::InvalidConfig("audio encoder has no input yet"))?;
        let in_samples = frame.map_or(0, |f| f.nb_samples());
        let cap = resampler.out_samples(in_samples).max(1);
        let mut tmp = RawFrame::new_audio(self.out_fmt, &self.out_layout, self.out_rate, cap)?;
        resampler.convert(frame, &mut tmp)?;
        if tmp.nb_samples() > 0 {
            self.fifo.write(&tmp)?;
        }
        Ok(())
    }

    /// Pull `frame_size` chunks (plus a final partial one when `final_flush`) out of the FIFO,
    /// encode each, and collect the packets.
    fn drain_fifo(&mut self, out: &mut Vec<Packet>, final_flush: bool) -> Result<()> {
        loop {
            let size = self.fifo.size();
            let take = if size >= self.frame_size {
                self.frame_size
            } else if final_flush && size > 0 {
                size
            } else {
                break;
            };

            let mut frame = RawFrame::new_audio(self.out_fmt, &self.out_layout, self.out_rate, self.frame_size)?;
            let got = self.fifo.read(&mut frame, take)?;
            frame.set_nb_samples(got);
            frame.set_pts(self.next_pts);
            self.next_pts += got as i64;

            self.ctx.send_frame(Some(&frame))?;
            self.recv_packets(out)?;
        }
        Ok(())
    }

    /// Drain every packet the encoder can currently produce.
    fn recv_packets(&mut self, out: &mut Vec<Packet>) -> Result<()> {
        loop {
            let received = self.ctx.receive_packet(&mut self.recv);
            match drain_received(received, || Ok(Packet::from_raw(self.recv.move_out()?))) {
                Some(pkt) => out.push(pkt?),
                None => return Ok(()),
            }
        }
    }
}

impl sealed::Encode for AudioEncoder {
    fn codec_ctx(&self) -> &CodecContext {
        &self.ctx
    }
    fn time_base(&self) -> Rational {
        self.time_base
    }
}
impl Encoder for AudioEncoder {}

/// Builder for an [`AudioEncoder`].
#[derive(Default)]
pub struct AudioEncoderBuilder {
    codec: Option<AudioCodec>,
    bitrate: Option<Bitrate>,
    sample_rate: Option<SampleRate>,
    channels: Option<Channels>,
    /// The decoder's true layout, inherited via [`from_decoder`](Self::from_decoder). Used only
    /// when the user didn't pin channels explicitly, so a non-canonical source layout (e.g. a
    /// 5.1 variant) is preserved verbatim instead of being re-canonicalized through `Channels`.
    layout: Option<ChannelLayout>,
    sample_format: Option<SampleFormat>,
    global_header: Option<bool>,
}

impl AudioEncoderBuilder {
    /// The codec to encode with (required).
    pub fn codec(mut self, codec: AudioCodec) -> Self {
        self.codec = Some(codec);
        self
    }

    /// Target bit rate (ignored by lossless codecs like FLAC).
    pub fn bitrate(mut self, bitrate: Bitrate) -> Self {
        self.bitrate = Some(bitrate);
        self
    }

    /// Output sample rate (defaults to the decoder's via [`from_decoder`], else 44.1 kHz).
    ///
    /// [`from_decoder`]: Self::from_decoder
    pub fn sample_rate(mut self, sample_rate: SampleRate) -> Self {
        self.sample_rate = Some(sample_rate);
        self
    }

    /// Output channel configuration (defaults to the decoder's, else stereo).
    pub fn channels(mut self, channels: Channels) -> Self {
        self.channels = Some(channels);
        self
    }

    /// The sample format to encode in (defaults to the encoder's preferred format).
    pub fn sample_format(mut self, sample_format: SampleFormat) -> Self {
        self.sample_format = Some(sample_format);
        self
    }

    /// Inherit sample rate and channel layout from a decoder — the common way to keep the
    /// input's audio parameters while changing codec/bitrate.
    pub fn from_decoder(mut self, decoder: &crate::codec::decoder::Decoder) -> Self {
        let rate = decoder.sample_rate();
        if self.sample_rate.is_none() && rate > 0 {
            self.sample_rate = Some(SampleRate::Hz(rate));
        }
        // Inherit the decoder's exact layout, not the lossy `Channels` enum, so the encoder
        // matches the layout the audio filter graph is fed. An explicit `.channels()` still wins
        // in `build()`.
        self.layout.get_or_insert_with(|| decoder.ch_layout_owned());
        self
    }

    /// Override the global-header flag (needed for MP4/MKV; on by default).
    pub fn global_header(mut self, enabled: bool) -> Self {
        self.global_header = Some(enabled);
        self
    }

    /// Validate the configuration, open the encoder, and return it.
    pub fn build(self) -> Result<AudioEncoder> {
        crate::log::ensure_init();
        let codec = self.codec.ok_or(Error::InvalidConfig("audio encoder requires a codec"))?;
        let av_codec = find_encoder_by_name(codec.encoder_name())?;
        let mut ctx = CodecContext::alloc(av_codec)?;

        let out_rate = self.sample_rate.unwrap_or(SampleRate::Hz44100).hz();
        // An explicit channel choice wins; otherwise use the decoder's inherited layout verbatim,
        // falling back to stereo when nothing was inherited.
        let out_layout = match self.channels {
            Some(ch) => ch.to_layout(),
            None => self.layout.unwrap_or_else(|| Channels::Stereo.to_layout()),
        };

        // Pick the sample format: the user's choice, else the encoder's first advertised format,
        // else Fltp (the common encoder input for AAC/Opus/Vorbis/MP3).
        let out_fmt = self.sample_format.map(SampleFormat::to_av).unwrap_or_else(|| {
            ctx.supported_sample_formats()
                .first()
                .copied()
                .unwrap_or(sys::AVSampleFormat_AV_SAMPLE_FMT_FLTP)
        });

        ctx.set_audio_format(out_rate, out_fmt, &out_layout);
        if let Some(b) = self.bitrate {
            ctx.set_bit_rate(b.as_bps());
        }
        let time_base = Rational::new(1, out_rate.max(1));
        ctx.set_time_base(time_base);
        ctx.set_threading(0, (sys::FF_THREAD_FRAME | sys::FF_THREAD_SLICE) as i32);
        if self.global_header.unwrap_or(true) {
            ctx.set_global_header();
        }
        ctx.open()?;

        // After open the encoder reports its required samples-per-frame; variable-size codecs
        // (FLAC, PCM) report 0, so we feed them a fixed chunk.
        let frame_size = if ctx.frame_size() > 0 && !ctx.accepts_variable_frame_size() { ctx.frame_size() } else { 4096 };

        let fifo = AudioFifo::new(out_fmt, out_layout.count(), frame_size)?;

        Ok(AudioEncoder {
            ctx,
            recv: RawPacket::alloc()?,
            time_base,
            out_fmt,
            out_rate,
            out_layout,
            resampler: None,
            fifo,
            frame_size,
            next_pts: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Resolution validation runs before any codec lookup, so these assert the typed error
    // without needing a real encoder.
    #[test]
    fn zero_resolution_is_unsupported() {
        // `matches!` on the `Result` avoids needing `Debug` on the `Ok` (encoder) type.
        let result = VideoEncoder::builder().codec(VideoCodec::H264).resolution(0, 720).build();
        assert!(matches!(result, Err(Error::UnsupportedResolution { width: 0, height: 720 })));
    }

    #[test]
    fn out_of_i32_range_resolution_is_unsupported_not_wrapped() {
        // A `u32` above `i32::MAX` must not wrap through to a bogus (possibly negative)
        // dimension; it is reported as unsupported with a saturated width.
        let result = VideoEncoder::builder().codec(VideoCodec::H264).resolution(u32::MAX, 480).build();
        assert!(matches!(result, Err(Error::UnsupportedResolution { width: i32::MAX, height: 480 })));
    }
}
