//! Tier 2 (builder) and Tier 1 (one-liner) transcoding, layered over the frame-level API.

pub mod config;
mod pipeline;
pub mod progress;

mod oneliner;

pub use config::{AudioConfig, AudioConfigBuilder, VideoConfig, VideoConfigBuilder};
pub use oneliner::{TranscodeJob, transcode};
pub use progress::{Progress, TranscodeSummary};

use crate::error::{Error, Result};
use crate::filter::{AudioFilterChain, VideoFilterChain};
use config::{AudioConfig as AConfig, VideoConfig as VConfig};
use pipeline::TranscodeOptions;
use std::ops::RangeInclusive;
use std::time::Duration;

/// A configured transcode job. Cheap to hold and re-runnable.
///
/// ```no_run
/// use media::prelude::*;
/// # fn demo() -> media::Result<()> {
/// let job = Transcoder::builder()
///     .input("input.mp4")
///     .output("output.mp4")
///     .video(VideoConfig::builder().codec(VideoCodec::H264).build()?)
///     .build()?;
/// job.run()?;
/// # Ok(()) }
/// ```
pub struct Transcoder {
    opts: TranscodeOptions,
}

impl Transcoder {
    /// Start building a transcode.
    pub fn builder() -> TranscoderBuilder {
        TranscoderBuilder::default()
    }

    /// Run the transcode to completion.
    pub fn run(&self) -> Result<TranscodeSummary> {
        pipeline::run(&self.opts, |_| {})
    }

    /// Run the transcode, invoking `on_progress` as media is processed.
    pub fn run_with_progress(&self, on_progress: impl FnMut(Progress)) -> Result<TranscodeSummary> {
        pipeline::run(&self.opts, on_progress)
    }
}

/// Builder for a [`Transcoder`].
#[derive(Default)]
pub struct TranscoderBuilder {
    input: Option<String>,
    output: Option<String>,
    video: Option<VConfig>,
    audio: Option<AConfig>,
    drop_video: bool,
    drop_audio: bool,
    trim: Option<(f64, f64)>,
    video_filter: VideoFilterChain,
    audio_filter: AudioFilterChain,
}

impl TranscoderBuilder {
    /// The input file (required).
    pub fn input(mut self, path: impl Into<String>) -> Self {
        self.input = Some(path.into());
        self
    }

    /// The output file (required). The container is inferred from the extension.
    pub fn output(mut self, path: impl Into<String>) -> Self {
        self.output = Some(path.into());
        self
    }

    /// How to encode the video stream. Omit to inherit the input's geometry with H.264.
    pub fn video(mut self, config: VideoConfig) -> Self {
        self.video = Some(config);
        self
    }

    /// How to encode the audio stream. Omit to stream-copy the input audio (re-encoding only
    /// when the source codec can't go into the target container).
    pub fn audio(mut self, config: AudioConfig) -> Self {
        self.audio = Some(config);
        self
    }

    /// Drop the video stream from the output.
    pub fn drop_video(mut self) -> Self {
        self.drop_video = true;
        self
    }

    /// Drop the audio stream from the output.
    pub fn drop_audio(mut self) -> Self {
        self.drop_audio = true;
        self
    }

    /// Keep only the given time range, re-based to start at zero.
    pub fn trim(mut self, range: RangeInclusive<Duration>) -> Self {
        self.trim = Some((range.start().as_secs_f64(), range.end().as_secs_f64()));
        self
    }

    /// Apply a video filter chain.
    pub fn video_filter(mut self, filter: VideoFilterChain) -> Self {
        self.video_filter = filter;
        self
    }

    /// Apply an audio filter chain. Setting one forces the audio stream to be re-encoded.
    pub fn audio_filter(mut self, filter: AudioFilterChain) -> Self {
        self.audio_filter = filter;
        self
    }

    /// Validate and produce the [`Transcoder`].
    pub fn build(self) -> Result<Transcoder> {
        Ok(Transcoder {
            opts: TranscodeOptions {
                input: self.input.ok_or(Error::InvalidConfig("transcoder requires an input"))?,
                output: self.output.ok_or(Error::InvalidConfig("transcoder requires an output"))?,
                video: self.video,
                audio: self.audio,
                drop_video: self.drop_video,
                drop_audio: self.drop_audio,
                trim: self.trim,
                video_filter: self.video_filter,
                audio_filter: self.audio_filter,
            },
        })
    }
}
