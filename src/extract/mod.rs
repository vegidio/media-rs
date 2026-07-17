//! Frame extraction: turn a video into a set of images.
//!
//! Three tiers, mirroring the transcode API so there's no new mental model:
//! - **Tier 1** — the [`extract_frames`] one-liner.
//! - **Tier 2** — the [`FrameExtractor`] builder (interval, format, resolution, naming, range,
//!   output target, progress).
//! - **Tier 3** — [`MediaReader::stream`](crate::format::MediaReader::stream)`(idx)`
//!   [`.sampled_at(..)`](crate::format::StreamRef::sampled_at) for frame-by-frame control.
//!
//! ```no_run
//! use media::prelude::*;
//! use std::time::Duration;
//!
//! // Tier 1: one JPEG per second into a directory.
//! extract_frames("input.mp4")
//!     .every(Duration::from_secs(1))
//!     .to_dir("frames/")
//!     .run()?;
//!
//! // Tier 2: exactly 20 PNGs evenly spread, scaled, kept in memory.
//! let report = FrameExtractor::builder()
//!     .input("input.mp4")
//!     .interval(Interval::Count(20))
//!     .format(ImageFormat::Png)
//!     .resolution(Resolution::Fixed(640, 360))
//!     .to_memory()
//!     .build()?
//!     .run()?;
//! println!("{} frames in {:?}", report.frame_count(), report.elapsed());
//! # Ok::<(), media::Error>(())
//! ```
//!
//! Sampling is seek-based: only the frames each sample point needs are decoded, so pulling a
//! frame every 10s from a two-hour video does not decode the whole stream.

pub mod frame;
pub mod report;
pub(crate) mod sampler;

mod encode_pool;
mod oneliner;
mod types;

pub use frame::ExtractedFrame;
pub use oneliner::{ExtractJob, extract_frames};
pub use report::ExtractReport;
pub use sampler::SampledFrames;
pub use types::{ImageFormat, Interval, NamingScheme, Output, Resolution};

use crate::error::{Error, Result};
use crate::format::MediaReader;
use crate::transcode::Progress;
use crate::types::stream_kind::StreamKind;
use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// A configured frame-extraction job.
///
/// ```no_run
/// use media::prelude::*;
/// # fn demo() -> media::Result<()> {
/// let report = FrameExtractor::builder()
///     .input("input.mp4")
///     .interval(Interval::EverySeconds(1.0))
///     .format(ImageFormat::Jpeg { quality: 90 })
///     .output_dir("frames/")
///     .build()?
///     .run()?;
/// println!("extracted {} frames", report.frame_count());
/// # Ok(()) }
/// ```
pub struct FrameExtractor {
    opts: ExtractOptions,
}

/// The resolved set of options an extraction runs with.
struct ExtractOptions {
    input: String,
    interval: Interval,
    format: ImageFormat,
    resolution: Resolution,
    naming: NamingScheme,
    range: Option<(Duration, Duration)>,
    output: Output,
}

impl FrameExtractor {
    /// Start building an extraction.
    pub fn builder() -> FrameExtractorBuilder {
        FrameExtractorBuilder::default()
    }

    /// Run the extraction to completion.
    pub fn run(self) -> Result<ExtractReport> {
        run_extraction(self.opts, |_| {})
    }

    /// Run the extraction, invoking `on_progress` as frames are produced. Reuses the same
    /// [`Progress`] type as the transcoder.
    pub fn run_with_progress(self, on_progress: impl FnMut(Progress)) -> Result<ExtractReport> {
        run_extraction(self.opts, on_progress)
    }
}

/// Builder for a [`FrameExtractor`].
#[derive(Default)]
pub struct FrameExtractorBuilder {
    input: Option<String>,
    interval: Option<Interval>,
    format: Option<ImageFormat>,
    resolution: Option<Resolution>,
    naming: Option<NamingScheme>,
    range: Option<(Duration, Duration)>,
    output: Option<Output>,
}

impl FrameExtractorBuilder {
    /// The input file (required).
    pub fn input(mut self, path: impl Into<String>) -> Self {
        self.input = Some(path.into());
        self
    }

    /// How often to sample frames (required).
    pub fn interval(mut self, interval: Interval) -> Self {
        self.interval = Some(interval);
        self
    }

    /// The output image format (defaults to JPEG at quality 90).
    pub fn format(mut self, format: ImageFormat) -> Self {
        self.format = Some(format);
        self
    }

    /// The output resolution (defaults to [`Resolution::Original`]).
    pub fn resolution(mut self, resolution: Resolution) -> Self {
        self.resolution = Some(resolution);
        self
    }

    /// How output files are named (defaults to `frame_0000`, `frame_0001`, …). Only applies to
    /// the directory output.
    pub fn naming(mut self, naming: NamingScheme) -> Self {
        self.naming = Some(naming);
        self
    }

    /// Restrict extraction to this time range of the source.
    pub fn range(mut self, range: RangeInclusive<Duration>) -> Self {
        self.range = Some((*range.start(), *range.end()));
        self
    }

    /// Set the output target explicitly.
    pub fn output(mut self, output: Output) -> Self {
        self.output = Some(output);
        self
    }

    /// Write frames as image files into `dir` (created if missing).
    pub fn output_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.output = Some(Output::Directory(dir.into()));
        self
    }

    /// Keep every extracted frame in memory; read them from
    /// [`ExtractReport::frames`].
    pub fn to_memory(mut self) -> Self {
        self.output = Some(Output::InMemory);
        self
    }

    /// Deliver each frame to `f` as it is produced, buffering nothing.
    pub fn to_callback(mut self, f: impl FnMut(ExtractedFrame) -> Result<()> + 'static) -> Self {
        self.output = Some(Output::Callback(Box::new(f)));
        self
    }

    /// Validate and produce the [`FrameExtractor`].
    pub fn build(self) -> Result<FrameExtractor> {
        Ok(FrameExtractor {
            opts: ExtractOptions {
                input: self.input.ok_or(Error::InvalidConfig("frame extractor requires an input"))?,
                interval: self.interval.ok_or(Error::InvalidConfig("frame extractor requires an interval"))?,
                format: self.format.unwrap_or_default(),
                resolution: self.resolution.unwrap_or_default(),
                naming: self.naming.unwrap_or_default(),
                range: self.range,
                output: self.output.ok_or(Error::InvalidConfig("frame extractor requires an output"))?,
            },
        })
    }
}

/// Open the input, sample it, and route each frame to the chosen output.
fn run_extraction(opts: ExtractOptions, mut on_progress: impl FnMut(Progress)) -> Result<ExtractReport> {
    let started = Instant::now();
    let mut reader = MediaReader::open(&opts.input)?;
    let video_idx = reader.best_stream(StreamKind::Video)?;

    // Bounds for the progress percentage.
    let duration_secs = reader.duration_secs();
    let (start_secs, span_secs) = match opts.range {
        Some((s, e)) => (s.as_secs_f64(), (e.as_secs_f64() - s.as_secs_f64()).max(0.0)),
        None => (0.0, duration_secs),
    };

    let sampler = SampledFrames::new(&mut reader, video_idx, opts.interval, opts.range, opts.resolution)?;

    // Prepare the output sink.
    let mut output = opts.output;
    if let Output::Directory(dir) = &output {
        std::fs::create_dir_all(dir).map_err(|e| Error::CreateOutput(format!("{}: {e}", dir.display())))?;
    }

    // The directory sink encodes+writes each frame, which we push onto a worker pool so it
    // overlaps decoding instead of stalling it. The other sinks do no per-frame encode work in
    // this loop (in-memory just stores raw RGB; the callback is `FnMut` and must stay ordered
    // on this thread), so they run inline.
    let mut pool = match &output {
        Output::Directory(_) => Some(encode_pool::EncodePool::new(opts.format)),
        _ => None,
    };

    let mut collected = Vec::new();
    let mut count: u64 = 0;

    for item in sampler {
        let frame = item?;
        let ts_secs = frame.timestamp().as_secs_f64();

        match &mut output {
            Output::Directory(dir) => {
                let name = opts.naming.file_name(frame.index(), opts.format.extension());
                let path = dir.join(name);
                pool.as_ref().expect("directory output has a pool").submit(frame, path)?;
            }
            Output::InMemory => collected.push(frame),
            Output::Callback(cb) => cb(frame)?,
        }

        count += 1;
        on_progress(Progress::new((ts_secs - start_secs).max(0.0), span_secs, count, started));
    }

    // Drain the pool before reporting, so a write error surfaces and `elapsed()` covers it.
    if let Some(pool) = pool.take() {
        pool.finish()?;
    }

    Ok(ExtractReport::new(count, started.elapsed(), collected))
}
