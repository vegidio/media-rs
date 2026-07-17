//! Audio resampling: convert decoded [`Frame`]s between sample formats, sample rates, and
//! channel layouts.

use crate::error::{Error, Result};
use crate::frame::Frame;
use crate::raw::frame::RawFrame;
use crate::raw::resampler::ResampleContext;
use crate::types::channel_layout::{ChannelLayout, Channels};
use crate::types::sample_format::SampleFormat;
use crate::types::audio::SampleRate;

/// A configured audio resampler for one fixed (input → output) format.
///
/// Build one with [`Resampler::builder`], then feed decoded frames to
/// [`convert`](Self::convert). At end of stream call [`flush`](Self::flush) to drain any
/// samples the converter buffered internally.
///
/// ```no_run
/// use media::prelude::*;
/// # fn demo(frame: &Frame) -> media::Result<()> {
/// let mut resampler = Resampler::builder()
///     .input_format(SampleFormat::S16, SampleRate::Hz44100, Channels::Stereo)
///     .output_format(SampleFormat::Fltp, SampleRate::Hz48000, Channels::Mono)
///     .build()?;
/// let out = resampler.convert(frame)?;
/// # let _ = out; Ok(()) }
/// ```
pub struct Resampler {
    ctx: ResampleContext,
    out_fmt: SampleFormat,
    out_rate: i32,
    out_layout: ChannelLayout,
}

impl Resampler {
    /// Start configuring a resampler.
    pub fn builder() -> ResamplerBuilder {
        ResamplerBuilder::default()
    }

    /// Convert `frame` (which must match the configured input format) to the output format.
    /// The returned frame carries the resampled samples; its `sample_count` may differ from the
    /// input when the sample rate changes.
    pub fn convert(&mut self, frame: &Frame) -> Result<Frame> {
        self.drain(Some(frame))
    }

    /// Drain any samples still buffered inside the converter at end of stream. The returned
    /// frame may contain zero samples if nothing was buffered.
    #[must_use = "the flush frame carries the resampler's trailing samples"]
    pub fn flush(&mut self) -> Result<Frame> {
        self.drain(None)
    }

    fn drain(&mut self, frame: Option<&Frame>) -> Result<Frame> {
        let in_samples = frame.map_or(0, |f| f.raw.nb_samples());
        let cap = self.ctx.out_samples(in_samples).max(1);
        let mut out = RawFrame::new_audio(self.out_fmt.to_av(), &self.out_layout, self.out_rate, cap)?;
        self.ctx.convert(frame.map(|f| &f.raw), &mut out)?;
        Ok(Frame::from_raw(out))
    }
}

/// Builder for a [`Resampler`].
#[derive(Default)]
pub struct ResamplerBuilder {
    input: Option<(SampleFormat, SampleRate, Channels)>,
    output: Option<(SampleFormat, SampleRate, Channels)>,
}

impl ResamplerBuilder {
    /// The format of the frames you'll feed in (required).
    pub fn input_format(mut self, fmt: SampleFormat, rate: SampleRate, channels: Channels) -> Self {
        self.input = Some((fmt, rate, channels));
        self
    }

    /// The format to convert to (required).
    pub fn output_format(mut self, fmt: SampleFormat, rate: SampleRate, channels: Channels) -> Self {
        self.output = Some((fmt, rate, channels));
        self
    }

    /// Validate and build the resampler.
    pub fn build(self) -> Result<Resampler> {
        let (in_fmt, in_rate, in_ch) = self.input.ok_or(Error::InvalidConfig("resampler requires an input format"))?;
        let (out_fmt, out_rate, out_ch) =
            self.output.ok_or(Error::InvalidConfig("resampler requires an output format"))?;
        let in_layout = in_ch.to_layout();
        let out_layout = out_ch.to_layout();
        let ctx = ResampleContext::new(
            in_fmt.to_av(),
            in_rate.hz(),
            &in_layout,
            out_fmt.to_av(),
            out_rate.hz(),
            &out_layout,
        )?;
        Ok(Resampler { ctx, out_fmt, out_rate: out_rate.hz(), out_layout })
    }
}
