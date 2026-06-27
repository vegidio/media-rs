//! Tier 1: the one-liner `transcode(input).to(output).run()` API.

use super::config::VideoConfig;
use super::progress::{Progress, TranscodeSummary};
use super::{Transcoder, TranscoderBuilder};
use crate::error::Result;
use crate::filter::FilterChain;
use std::ops::RangeInclusive;

/// Begin a one-liner transcode from `input`. Chain [`to`](TranscodeJob::to) and
/// [`run`](TranscodeJob::run):
///
/// ```no_run
/// use media::prelude::*;
/// transcode("input.mp4").to("output.webm").run()?;
/// # Ok::<(), media::Error>(())
/// ```
pub fn transcode(input: impl Into<String>) -> TranscodeJob {
    TranscodeJob {
        input: input.into(),
        output: None,
        video: None,
        drop_video: false,
        drop_audio: false,
        trim: None,
        filter: FilterChain::new(),
    }
}

/// A fluent one-liner transcode. Inherits codecs/geometry from the input and the output
/// container by default; the methods here cover the common quick edits.
pub struct TranscodeJob {
    input: String,
    output: Option<String>,
    video: Option<VideoConfig>,
    drop_video: bool,
    drop_audio: bool,
    trim: Option<(f64, f64)>,
    filter: FilterChain,
}

impl TranscodeJob {
    /// Set the output file (required). Container/codecs are inferred from its extension.
    pub fn to(mut self, output: impl Into<String>) -> Self {
        self.output = Some(output.into());
        self
    }

    /// Drop the video stream (e.g. extracting audio).
    pub fn drop_video(mut self) -> Self {
        self.drop_video = true;
        self
    }

    /// Drop the audio stream.
    pub fn drop_audio(mut self) -> Self {
        self.drop_audio = true;
        self
    }

    /// Keep only the given time range (seconds).
    pub fn trim(mut self, range: RangeInclusive<f64>) -> Self {
        self.trim = Some((*range.start(), *range.end()));
        self
    }

    /// Override video encoding settings.
    pub fn video(mut self, config: VideoConfig) -> Self {
        self.video = Some(config);
        self
    }

    /// Apply a video filter chain.
    pub fn video_filter(mut self, filter: FilterChain) -> Self {
        self.filter = filter;
        self
    }

    fn into_transcoder(self) -> Result<Transcoder> {
        let mut b = TranscoderBuilder::default()
            .input(self.input)
            .drop_video_if(self.drop_video)
            .drop_audio_if(self.drop_audio)
            .video_filter(self.filter);
        if let Some(o) = self.output {
            b = b.output(o);
        }
        if let Some(v) = self.video {
            b = b.video(v);
        }
        if let Some((s, e)) = self.trim {
            b = b.trim(s..=e);
        }
        b.build()
    }

    /// Run the transcode to completion.
    pub fn run(self) -> Result<TranscodeSummary> {
        self.into_transcoder()?.run()
    }

    /// Run the transcode, reporting progress.
    pub fn run_with_progress(
        self,
        on_progress: impl FnMut(Progress),
    ) -> Result<TranscodeSummary> {
        self.into_transcoder()?.run_with_progress(on_progress)
    }
}
