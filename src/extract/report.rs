//! The outcome of a completed frame-extraction run.

use crate::extract::frame::ExtractedFrame;
use std::time::Duration;

/// What a [`FrameExtractor`](crate::extract::FrameExtractor) run produced.
pub struct ExtractReport {
    frame_count: u64,
    elapsed: Duration,
    frames: Vec<ExtractedFrame>,
}

impl ExtractReport {
    pub(crate) fn new(frame_count: u64, elapsed: Duration, frames: Vec<ExtractedFrame>) -> Self {
        Self {
            frame_count,
            elapsed,
            frames,
        }
    }

    /// How many frames were extracted.
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Wall-clock time the run took.
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// The extracted frames, when the output was
    /// [`Output::InMemory`](crate::extract::Output::InMemory). Empty for the directory and
    /// callback outputs, which don't buffer.
    pub fn frames(&self) -> &[ExtractedFrame] {
        &self.frames
    }

    /// Take ownership of the in-memory frames, leaving the report's list empty.
    pub fn into_frames(self) -> Vec<ExtractedFrame> {
        self.frames
    }
}
