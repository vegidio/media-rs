//! Tier 1: the one-liner `transcode(input).to(output).run()` API.

use super::TranscoderBuilder;
use super::config::VideoConfig;
use super::progress::{Progress, TranscodeSummary};
use crate::error::Result;
use crate::filter::FilterChain;
use std::ops::RangeInclusive;
use std::time::Duration;

/// Begin a one-liner transcode from `input`. Chain [`to`](TranscodeJob::to) and
/// [`run`](TranscodeJob::run):
///
/// ```no_run
/// use media::prelude::*;
/// transcode("input.mp4").to("output.webm").run()?;
/// # Ok::<(), media::Error>(())
/// ```
pub fn transcode(input: impl Into<String>) -> TranscodeJob {
    TranscodeJob { builder: TranscoderBuilder::default().input(input) }
}

/// A fluent one-liner transcode. Inherits codecs/geometry from the input and the output
/// container by default; the methods here cover the common quick edits. It is a thin facade
/// over [`TranscoderBuilder`].
pub struct TranscodeJob {
    builder: TranscoderBuilder,
}

impl TranscodeJob {
    /// Set the output file (required). Container/codecs are inferred from its extension.
    pub fn to(mut self, output: impl Into<String>) -> Self {
        self.builder = self.builder.output(output);
        self
    }

    /// Drop the video stream (e.g. extracting audio).
    pub fn drop_video(mut self) -> Self {
        self.builder = self.builder.drop_video();
        self
    }

    /// Drop the audio stream.
    pub fn drop_audio(mut self) -> Self {
        self.builder = self.builder.drop_audio();
        self
    }

    /// Keep only the given time range.
    pub fn trim(mut self, range: RangeInclusive<Duration>) -> Self {
        self.builder = self.builder.trim(range);
        self
    }

    /// Override video encoding settings.
    pub fn video(mut self, config: VideoConfig) -> Self {
        self.builder = self.builder.video(config);
        self
    }

    /// Apply a video filter chain.
    pub fn video_filter(mut self, filter: FilterChain) -> Self {
        self.builder = self.builder.video_filter(filter);
        self
    }

    /// Run the transcode to completion.
    pub fn run(self) -> Result<TranscodeSummary> {
        self.builder.build()?.run()
    }

    /// Run the transcode, reporting progress.
    pub fn run_with_progress(self, on_progress: impl FnMut(Progress)) -> Result<TranscodeSummary> {
        self.builder.build()?.run_with_progress(on_progress)
    }
}
