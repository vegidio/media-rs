//! Composable, typed video filters.
//!
//! [`FilterChain`] builds a libavfilter description from chainable, strongly-typed operators
//! so callers rarely touch raw filtergraph strings. For anything not covered, drop down to
//! [`FilterChain::raw`].

use crate::error::Result;
use crate::frame::Frame;
use crate::raw::codec_context::Receive;
use crate::raw::filter_graph::{VideoFilterGraph, VideoInput};
use crate::raw::frame::RawFrame;
use crate::types::pixel_format::PixelFormat;
use crate::types::rational::Rational;

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
        Self {
            brightness: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            gamma: 1.0,
        }
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
pub struct FilterChain {
    stages: Vec<String>,
}

impl FilterChain {
    /// An empty chain (a no-op).
    pub fn new() -> Self {
        Self::default()
    }

    /// A chain from a raw libavfilter string, e.g. `"scale=1280:720,unsharp=5:5:1.0"`.
    pub fn raw(description: impl Into<String>) -> Self {
        Self {
            stages: vec![description.into()],
        }
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

/// A built, runnable video filter graph for frames of a fixed input shape. Used internally
/// by the transcode pipeline.
pub(crate) struct VideoFilter {
    graph: VideoFilterGraph,
    out: RawFrame,
}

impl VideoFilter {
    pub(crate) fn new(
        width: i32,
        height: i32,
        pix_fmt: PixelFormat,
        time_base: Rational,
        sample_aspect_ratio: Rational,
        chain: &FilterChain,
    ) -> Result<Self> {
        let input = VideoInput {
            width,
            height,
            pix_fmt: pix_fmt.to_av(),
            time_base,
            sample_aspect_ratio,
        };
        Ok(Self {
            graph: VideoFilterGraph::new(&input, &chain.description())?,
            out: RawFrame::alloc()?,
        })
    }

    /// The width of frames this filter emits.
    pub(crate) fn output_width(&self) -> i32 {
        self.graph.out_width()
    }

    /// The height of frames this filter emits.
    pub(crate) fn output_height(&self) -> i32 {
        self.graph.out_height()
    }

    /// The pixel format of frames this filter emits.
    pub(crate) fn output_pixel_format(&self) -> PixelFormat {
        PixelFormat::from_av(self.graph.out_pix_fmt())
    }

    /// Push a frame and collect every frame the graph emits in response. Consumes the
    /// frame: `av_buffersrc_add_frame` takes ownership of its reference.
    pub(crate) fn filter(&mut self, mut frame: Frame) -> Result<Vec<Frame>> {
        self.graph.push(Some(&mut frame.raw))?;
        self.drain()
    }

    /// Signal end of stream and collect any remaining frames.
    pub(crate) fn flush(&mut self) -> Result<Vec<Frame>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_chain_has_empty_description() {
        let chain = FilterChain::new();
        assert!(chain.is_empty());
        assert_eq!(chain.description(), "");
    }

    #[test]
    fn raw_chain_is_used_verbatim() {
        let chain = FilterChain::raw("scale=1280:720,unsharp=5:5:1.0");
        assert!(!chain.is_empty());
        assert_eq!(chain.description(), "scale=1280:720,unsharp=5:5:1.0");
    }

    #[test]
    fn operators_compose_in_order_joined_by_commas() {
        let chain = FilterChain::new()
            .scale(640, 360)
            .fps(30)
            .denoise(DenoiseLevel::Moderate);
        assert_eq!(chain.description(), "scale=640:360,fps=30,hqdn3d=4:4:9:9");
    }

    #[test]
    fn denoise_levels_map_to_distinct_strengths() {
        assert_eq!(
            FilterChain::new().denoise(DenoiseLevel::Light).description(),
            "hqdn3d=1.5:1.5:6:6"
        );
        assert_eq!(
            FilterChain::new().denoise(DenoiseLevel::Heavy).description(),
            "hqdn3d=8:8:12:12"
        );
    }

    #[test]
    fn color_correct_emits_an_eq_filter_with_all_knobs() {
        // Defaults are identity; only the knobs touched should move away from them.
        let chain = FilterChain::new().color_correct(|c| c.brightness(0.1).contrast(1.2));
        assert_eq!(
            chain.description(),
            "eq=brightness=0.1:contrast=1.2:saturation=1:gamma=1"
        );

        let full = FilterChain::new()
            .color_correct(|c| c.brightness(-0.2).contrast(0.9).saturation(1.5).gamma(0.8));
        assert_eq!(
            full.description(),
            "eq=brightness=-0.2:contrast=0.9:saturation=1.5:gamma=0.8"
        );
    }
}
