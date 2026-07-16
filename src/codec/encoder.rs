//! Video encoding: turn decoded [`Frame`]s back into compressed [`Packet`]s.

use crate::error::{Error, Result};
use crate::frame::Frame;
use crate::packet::Packet;
use crate::raw::codec_context::{CodecContext, drain_received, find_encoder_by_name};
use crate::raw::packet::RawPacket;
use crate::types::codec::VideoCodec;
use crate::types::pixel_format::PixelFormat;
use crate::types::preset::H264Preset;
use crate::types::profile::H264Profile;
use crate::types::rational::{Bitrate, Framerate, Rational};

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

    pub(crate) fn codec_ctx(&self) -> &CodecContext {
        &self.ctx
    }
}

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
        let codec = self
            .codec
            .ok_or(Error::InvalidConfig("video encoder requires a codec"))?;
        let width = self
            .width
            .ok_or(Error::InvalidConfig("video encoder requires a resolution"))?;
        let height = self
            .height
            .ok_or(Error::InvalidConfig("video encoder requires a resolution"))?;
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
        Ok(VideoEncoder {
            ctx,
            recv: RawPacket::alloc()?,
            time_base,
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
        assert!(matches!(
            result,
            Err(Error::UnsupportedResolution { width: 0, height: 720 })
        ));
    }

    #[test]
    fn out_of_i32_range_resolution_is_unsupported_not_wrapped() {
        // A `u32` above `i32::MAX` must not wrap through to a bogus (possibly negative)
        // dimension; it is reported as unsupported with a saturated width.
        let result = VideoEncoder::builder()
            .codec(VideoCodec::H264)
            .resolution(u32::MAX, 480)
            .build();
        assert!(matches!(
            result,
            Err(Error::UnsupportedResolution {
                width: i32::MAX,
                height: 480
            })
        ));
    }
}
