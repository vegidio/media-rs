//! Tier 1: the `extract_frames(input).every(dur).to_dir(dir).run()` one-liner.

use super::FrameExtractorBuilder;
use super::report::ExtractReport;
use super::types::Interval;
use crate::error::Result;
use crate::transcode::Progress;
use std::path::PathBuf;
use std::time::Duration;

/// Begin a one-liner extraction from `input`. Chain an interval ([`every`](ExtractJob::every)
/// or [`interval`](ExtractJob::interval)) and an output ([`to_dir`](ExtractJob::to_dir)), then
/// [`run`](ExtractJob::run):
///
/// ```no_run
/// use media::prelude::*;
/// use std::time::Duration;
/// extract_frames("input.mp4")
///     .every(Duration::from_secs(1))
///     .to_dir("frames/")
///     .run()?;
/// # Ok::<(), media::Error>(())
/// ```
pub fn extract_frames(input: impl Into<String>) -> ExtractJob {
    ExtractJob {
        builder: FrameExtractorBuilder::default().input(input),
    }
}

/// A fluent one-liner frame extraction. Defaults to JPEG output; it is a thin facade over
/// [`FrameExtractorBuilder`].
pub struct ExtractJob {
    builder: FrameExtractorBuilder,
}

impl ExtractJob {
    /// Extract one frame every `interval` of video time.
    pub fn every(mut self, interval: Duration) -> Self {
        self.builder = self.builder.interval(Interval::EverySeconds(interval.as_secs_f64()));
        self
    }

    /// Set an explicit sampling [`Interval`] (e.g. `Interval::Count(20)`).
    pub fn interval(mut self, interval: Interval) -> Self {
        self.builder = self.builder.interval(interval);
        self
    }

    /// Write frames as image files into `dir` (created if missing).
    pub fn to_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.builder = self.builder.output_dir(dir);
        self
    }

    /// Run the extraction to completion.
    pub fn run(self) -> Result<ExtractReport> {
        self.builder.build()?.run()
    }

    /// Run the extraction, reporting progress.
    pub fn run_with_progress(self, on_progress: impl FnMut(Progress)) -> Result<ExtractReport> {
        self.builder.build()?.run_with_progress(on_progress)
    }
}
