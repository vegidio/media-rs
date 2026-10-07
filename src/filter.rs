//! Composable, typed audio and video filters.
//!
//! [`VideoFilterChain`] and [`AudioFilterChain`] build a libavfilter description from chainable,
//! strongly-typed operators so callers rarely touch raw filtergraph strings. For anything not
//! covered, drop down to their `raw` constructors.

use crate::codec::Decoder;
use crate::error::Result;
use crate::frame::Frame;
use crate::raw::codec_context::Receive;
use crate::raw::filter_graph::{AudioFilterGraph, AudioInput, VideoFilterGraph, VideoInput};
use crate::raw::frame::RawFrame;
use crate::types::channel_layout::ChannelLayout;
use crate::types::pixel_format::PixelFormat;
use crate::types::rational::{Framerate, Rational};
use crate::types::sample_format::SampleFormat;
use std::time::Duration;

/// A gain amount in decibels (e.g. `Decibels(-6.0)` halves perceived loudness).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Decibels(pub f64);

/// How aggressively to denoise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenoiseLevel {
    /// Subtle.
    Light,
    /// Balanced.
    Moderate,
    /// Strong.
    Heavy,
}

impl DenoiseLevel {
    fn hqdn3d(self) -> &'static str {
        match self {
            DenoiseLevel::Light => "hqdn3d=1.5:1.5:6:6",
            DenoiseLevel::Moderate => "hqdn3d=4:4:9:9",
            DenoiseLevel::Heavy => "hqdn3d=8:8:12:12",
        }
    }
}

/// Color adjustment knobs, applied via the `eq` filter.
#[derive(Debug, Clone, Copy)]
pub struct ColorCorrect {
    brightness: f64,
    contrast: f64,
    saturation: f64,
    gamma: f64,
}

impl Default for ColorCorrect {
    fn default() -> Self {
        Self { brightness: 0.0, contrast: 1.0, saturation: 1.0, gamma: 1.0 }
    }
}

impl ColorCorrect {
    /// Brightness shift in `[-1.0, 1.0]` (0 = unchanged).
    pub fn brightness(mut self, v: f64) -> Self {
        self.brightness = v;
        self
    }

    /// Contrast multiplier (1.0 = unchanged).
    pub fn contrast(mut self, v: f64) -> Self {
        self.contrast = v;
        self
    }

    /// Saturation multiplier (1.0 = unchanged).
    pub fn saturation(mut self, v: f64) -> Self {
        self.saturation = v;
        self
    }

    /// Gamma (1.0 = unchanged).
    pub fn gamma(mut self, v: f64) -> Self {
        self.gamma = v;
        self
    }

    fn to_filter(self) -> String {
        format!(
            "eq=brightness={}:contrast={}:saturation={}:gamma={}",
            self.brightness, self.contrast, self.saturation, self.gamma
        )
    }
}

/// A chain of video filters. Empty by default; each operator appends a stage.
#[derive(Debug, Clone, Default)]
pub struct VideoFilterChain {
    stages: Vec<String>,
}

impl VideoFilterChain {
    /// An empty chain (a no-op).
    pub fn new() -> Self {
        Self::default()
    }

    /// A chain from a raw libavfilter string, e.g. `"scale=1280:720,unsharp=5:5:1.0"`.
    pub fn raw(description: impl Into<String>) -> Self {
        Self { stages: vec![description.into()] }
    }

    /// Scale to `width`×`height`.
    pub fn scale(mut self, width: u32, height: u32) -> Self {
        self.stages.push(format!("scale={width}:{height}"));
        self
    }

    /// Force a constant frame rate.
    pub fn fps(mut self, fps: u32) -> Self {
        self.stages.push(format!("fps={fps}"));
        self
    }

    /// Denoise.
    pub fn denoise(mut self, level: DenoiseLevel) -> Self {
        self.stages.push(level.hqdn3d().to_owned());
        self
    }

    /// Color-correct via a closure over [`ColorCorrect`].
    pub fn color_correct(mut self, f: impl FnOnce(ColorCorrect) -> ColorCorrect) -> Self {
        self.stages.push(f(ColorCorrect::default()).to_filter());
        self
    }

    /// `true` if no stages were added.
    pub fn is_empty(&self) -> bool {
        self.stages.is_empty()
    }

    /// The combined libavfilter description (stages joined with `,`).
    pub fn description(&self) -> String {
        self.stages.join(",")
    }
}

/// The push/pull surface shared by the video and audio filter graphs, so the run loop
/// (`filter`/`flush`/`drain`) can be written once over either graph type.
trait RunnableGraph {
    fn push(&mut self, frame: Option<&mut RawFrame>) -> Result<()>;
    fn pull(&mut self, frame: &mut RawFrame) -> Result<Receive>;
}

impl RunnableGraph for VideoFilterGraph {
    fn push(&mut self, frame: Option<&mut RawFrame>) -> Result<()> {
        self.push(frame)
    }
    fn pull(&mut self, frame: &mut RawFrame) -> Result<Receive> {
        self.pull(frame)
    }
}

impl RunnableGraph for AudioFilterGraph {
    fn push(&mut self, frame: Option<&mut RawFrame>) -> Result<()> {
        self.push(frame)
    }
    fn pull(&mut self, frame: &mut RawFrame) -> Result<Receive> {
        self.pull(frame)
    }
}

/// The generic run loop over a filter graph plus its reusable output frame. The video and audio
/// filters both wrap one of these, so `filter`/`flush`/`drain` live in exactly one place.
struct FilterRunner<G: RunnableGraph> {
    graph: G,
    out: RawFrame,
}

impl<G: RunnableGraph> FilterRunner<G> {
    fn new(graph: G) -> Result<Self> {
        Ok(Self { graph, out: RawFrame::alloc()? })
    }

    /// Push a frame and collect every frame the graph emits in response. Consumes the frame:
    /// `av_buffersrc_add_frame` takes ownership of its reference.
    fn filter(&mut self, mut frame: Frame) -> Result<Vec<Frame>> {
        self.graph.push(Some(&mut frame.raw))?;
        self.drain()
    }

    /// Signal end of stream and collect any remaining frames.
    fn flush(&mut self) -> Result<Vec<Frame>> {
        self.graph.push(None)?;
        self.drain()
    }

    fn drain(&mut self) -> Result<Vec<Frame>> {
        let mut out = Vec::new();
        while let Receive::Got = self.graph.pull(&mut self.out)? {
            out.push(Frame::from_raw(self.out.move_out()?));
        }
        Ok(out)
    }
}

/// A [`VideoFilterChain`], built and ready to run on the frames of one decoder.
///
/// Build it from the [`Decoder`] whose frames it will take, which gives it their width, height, pixel format and
/// sample aspect ratio, and from their time base, usually the stream's. Then push each decoded frame through
/// [`filter`](Self::filter), and call [`flush`](Self::flush) at the end of the stream for any frames the chain still
/// holds. The `output_*` accessors tell you what comes out, before any frame goes in, which is what an encoder fed by
/// the filter needs:
///
/// ```no_run
/// use media::prelude::*;
/// # fn demo(reader: &mut MediaReader) -> media::Result<()> {
/// let index = reader.best_stream(StreamKind::Video)?;
/// let time_base = reader.stream_time_base(index)?;
/// let decoder = reader.stream(index).decoder()?;
///
/// let chain = VideoFilterChain::raw("scale=1280:720,format=yuv420p");
/// let mut filter = VideoFilter::new(&decoder, time_base, &chain)?;
/// assert_eq!((filter.output_width(), filter.output_height()), (1280, 720));
/// # Ok(()) }
/// ```
///
/// The input shape is fixed when the filter is built: a stream that changes resolution midway needs a new filter.
pub struct VideoFilter {
    runner: FilterRunner<VideoFilterGraph>,
}

impl VideoFilter {
    /// Build `chain` for the frames `decoder` produces, whose timestamps are in `time_base`. An empty chain passes
    /// frames through unchanged.
    ///
    /// Errors when the chain doesn't parse, names a filter this FFmpeg build lacks, or can't take frames of the
    /// decoder's shape.
    pub fn new(decoder: &Decoder, time_base: Rational, chain: &VideoFilterChain) -> Result<Self> {
        let (width, height) = (decoder.width() as i32, decoder.height() as i32);
        Self::from_shape(width, height, decoder.pixel_format(), time_base, decoder.sample_aspect_ratio(), chain)
    }

    /// Build `chain` for frames of an explicit shape, for callers inside the crate that have no decoder at hand.
    pub(crate) fn from_shape(
        width: i32,
        height: i32,
        pix_fmt: PixelFormat,
        time_base: Rational,
        sample_aspect_ratio: Rational,
        chain: &VideoFilterChain,
    ) -> Result<Self> {
        // An empty description isn't a valid graph; `null` is the pass-through filter.
        let description = if chain.is_empty() { "null".to_owned() } else { chain.description() };
        let input = VideoInput { width, height, pix_fmt: pix_fmt.to_av(), time_base, sample_aspect_ratio };
        Ok(Self { runner: FilterRunner::new(VideoFilterGraph::new(&input, &description)?)? })
    }

    /// The width of the frames this filter emits, in pixels.
    pub fn output_width(&self) -> u32 {
        self.runner.graph.out_width().max(0) as u32
    }

    /// The height of the frames this filter emits, in pixels.
    pub fn output_height(&self) -> u32 {
        self.runner.graph.out_height().max(0) as u32
    }

    /// The pixel format of the frames this filter emits.
    pub fn output_pixel_format(&self) -> PixelFormat {
        PixelFormat::from_av(self.runner.graph.out_pix_fmt())
    }

    /// The time base of the timestamps on the frames this filter emits. Most filters keep the input's; `fps` doesn't.
    pub fn output_time_base(&self) -> Rational {
        self.runner.graph.out_time_base()
    }

    /// The frame rate of the frames this filter emits, when the chain knows it (after `fps`, for example).
    pub fn output_frame_rate(&self) -> Option<Framerate> {
        let fr = self.runner.graph.out_frame_rate();
        (fr.num > 0 && fr.den > 0).then_some(Framerate(fr))
    }

    /// Push one frame through the chain and return every frame that comes out: usually one, none while a filter
    /// such as `fps` is holding frames back, several when it catches up.
    pub fn filter(&mut self, frame: Frame) -> Result<Vec<Frame>> {
        self.runner.filter(frame)
    }

    /// Signal the end of the stream and return the frames the chain was still holding.
    pub fn flush(&mut self) -> Result<Vec<Frame>> {
        self.runner.flush()
    }
}

/// A chain of audio filters. Empty by default; each operator appends a stage. The audio
/// counterpart to [`VideoFilterChain`].
#[derive(Debug, Clone, Default)]
pub struct AudioFilterChain {
    stages: Vec<String>,
}

impl AudioFilterChain {
    /// An empty chain (a no-op).
    pub fn new() -> Self {
        Self::default()
    }

    /// A chain from a raw libavfilter string, e.g. `"highpass=f=80,volume=2"`.
    pub fn raw(description: impl Into<String>) -> Self {
        Self { stages: vec![description.into()] }
    }

    /// Adjust the volume by a number of decibels (positive = louder).
    pub fn volume(mut self, gain: Decibels) -> Self {
        self.stages.push(format!("volume={}dB", gain.0));
        self
    }

    /// Alias for [`volume`](Self::volume).
    pub fn gain(self, gain: Decibels) -> Self {
        self.volume(gain)
    }

    /// Resample to a new sample rate.
    pub fn resample(mut self, rate: crate::types::audio::SampleRate) -> Self {
        self.stages.push(format!("aresample={}", rate.hz()));
        self
    }

    /// Remove content below `hz` (a high-pass filter — cuts rumble).
    pub fn highpass(mut self, hz: f64) -> Self {
        self.stages.push(format!("highpass=f={hz}"));
        self
    }

    /// Remove content above `hz` (a low-pass filter).
    pub fn lowpass(mut self, hz: f64) -> Self {
        self.stages.push(format!("lowpass=f={hz}"));
        self
    }

    /// Fade in from silence over `duration`, starting at the beginning.
    pub fn fade_in(mut self, duration: Duration) -> Self {
        self.stages.push(format!("afade=t=in:st=0:d={}", duration.as_secs_f64()));
        self
    }

    /// Fade out over `duration`. Begins at time `start` — pass the point where the fade should
    /// start (typically `stream_duration - duration`).
    pub fn fade_out(mut self, start: Duration, duration: Duration) -> Self {
        self.stages.push(format!("afade=t=out:st={}:d={}", start.as_secs_f64(), duration.as_secs_f64()));
        self
    }

    /// Change tempo without changing pitch (`0.5`–`2.0` per stage; chain for wider ranges).
    pub fn atempo(mut self, factor: f64) -> Self {
        self.stages.push(format!("atempo={factor}"));
        self
    }

    /// `true` if no stages were added.
    pub fn is_empty(&self) -> bool {
        self.stages.is_empty()
    }

    /// The combined libavfilter description (stages joined with `,`).
    pub fn description(&self) -> String {
        self.stages.join(",")
    }
}

/// An [`AudioFilterChain`], built and ready to run on the frames of one decoder: the audio counterpart to
/// [`VideoFilter`].
///
/// Build it from the [`Decoder`] whose frames it will take, which gives it their sample rate, sample format and channel
/// layout, and from their time base, usually the stream's. Push each decoded frame through [`filter`](Self::filter),
/// and call [`flush`](Self::flush) at the end of the stream. Raw libavfilter stages work too, such as `atrim` to cut
/// the audio at an exact sample, which whole decoded frames can't do:
///
/// ```no_run
/// use media::prelude::*;
/// # fn demo(reader: &mut MediaReader) -> media::Result<()> {
/// let index = reader.best_stream(StreamKind::Audio)?;
/// let time_base = reader.stream_time_base(index)?;
/// let decoder = reader.stream(index).decoder()?;
///
/// // Keep only what plays from 10.5 s on. Timestamp-based stages read each frame's `pts`.
/// let chain = AudioFilterChain::raw("atrim=start=10.5");
/// let mut filter = AudioFilter::new(&decoder, time_base, &chain)?;
/// # let _ = &mut filter; Ok(()) }
/// ```
///
/// The input shape is fixed when the filter is built: a stream that changes format midway needs a new filter.
pub struct AudioFilter {
    runner: FilterRunner<AudioFilterGraph>,
}

impl AudioFilter {
    /// Build `chain` for the frames `decoder` produces, whose timestamps are in `time_base`. An empty chain passes
    /// frames through unchanged.
    ///
    /// Errors when the chain doesn't parse, names a filter this FFmpeg build lacks, or can't take frames of the
    /// decoder's format.
    pub fn new(decoder: &Decoder, time_base: Rational, chain: &AudioFilterChain) -> Result<Self> {
        let rate = decoder.sample_rate() as i32;
        Self::from_shape(rate, decoder.sample_format(), decoder.ch_layout_owned(), time_base, chain)
    }

    /// Build `chain` for frames of an explicit shape, for callers inside the crate that have no decoder at hand.
    pub(crate) fn from_shape(
        sample_rate: i32,
        sample_fmt: SampleFormat,
        ch_layout: ChannelLayout,
        time_base: Rational,
        chain: &AudioFilterChain,
    ) -> Result<Self> {
        // An empty description isn't a valid graph; `anull` is the pass-through filter.
        let description = if chain.is_empty() { "anull".to_owned() } else { chain.description() };
        let input = AudioInput { sample_rate, sample_fmt: sample_fmt.to_av(), ch_layout, time_base };
        Ok(Self { runner: FilterRunner::new(AudioFilterGraph::new(&input, &description)?)? })
    }

    /// Push one frame through the chain and return every frame that comes out: none while a stage holds samples back,
    /// or when `atrim` drops the whole frame.
    pub fn filter(&mut self, frame: Frame) -> Result<Vec<Frame>> {
        self.runner.filter(frame)
    }

    /// Signal the end of the stream and return the frames the chain was still holding.
    pub fn flush(&mut self) -> Result<Vec<Frame>> {
        self.runner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_chain_has_empty_description() {
        let chain = VideoFilterChain::new();
        assert!(chain.is_empty());
        assert_eq!(chain.description(), "");
    }

    #[test]
    fn raw_chain_is_used_verbatim() {
        let chain = VideoFilterChain::raw("scale=1280:720,unsharp=5:5:1.0");
        assert!(!chain.is_empty());
        assert_eq!(chain.description(), "scale=1280:720,unsharp=5:5:1.0");
    }

    #[test]
    fn operators_compose_in_order_joined_by_commas() {
        let chain = VideoFilterChain::new().scale(640, 360).fps(30).denoise(DenoiseLevel::Moderate);
        assert_eq!(chain.description(), "scale=640:360,fps=30,hqdn3d=4:4:9:9");
    }

    #[test]
    fn denoise_levels_map_to_distinct_strengths() {
        assert_eq!(VideoFilterChain::new().denoise(DenoiseLevel::Light).description(), "hqdn3d=1.5:1.5:6:6");
        assert_eq!(VideoFilterChain::new().denoise(DenoiseLevel::Heavy).description(), "hqdn3d=8:8:12:12");
    }

    #[test]
    fn color_correct_emits_an_eq_filter_with_all_knobs() {
        // Defaults are identity; only the knobs touched should move away from them.
        let chain = VideoFilterChain::new().color_correct(|c| c.brightness(0.1).contrast(1.2));
        assert_eq!(chain.description(), "eq=brightness=0.1:contrast=1.2:saturation=1:gamma=1");

        let full =
            VideoFilterChain::new().color_correct(|c| c.brightness(-0.2).contrast(0.9).saturation(1.5).gamma(0.8));
        assert_eq!(full.description(), "eq=brightness=-0.2:contrast=0.9:saturation=1.5:gamma=0.8");
    }

    #[test]
    fn audio_chain_empty_by_default() {
        let chain = AudioFilterChain::new();
        assert!(chain.is_empty());
        assert_eq!(chain.description(), "");
    }

    #[test]
    fn audio_operators_compose_in_order() {
        let chain = AudioFilterChain::new().highpass(80.0).lowpass(15_000.0).volume(Decibels(3.0));
        assert_eq!(chain.description(), "highpass=f=80,lowpass=f=15000,volume=3dB");
    }

    #[test]
    fn audio_fades_and_resample() {
        use std::time::Duration;
        let chain = AudioFilterChain::new()
            .resample(crate::types::audio::SampleRate::Hz44100)
            .fade_in(Duration::from_secs(2))
            .fade_out(Duration::from_secs(27), Duration::from_secs(3));
        assert_eq!(chain.description(), "aresample=44100,afade=t=in:st=0:d=2,afade=t=out:st=27:d=3");
    }

    #[test]
    fn audio_raw_is_verbatim() {
        let chain = AudioFilterChain::raw("loudnorm=I=-16:TP=-1.5:LRA=11");
        assert!(!chain.is_empty());
        assert_eq!(chain.description(), "loudnorm=I=-16:TP=-1.5:LRA=11");
    }
}
