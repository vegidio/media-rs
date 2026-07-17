//! Transcode progress reporting.

use std::time::Instant;

/// A progress snapshot passed to the callback of
/// [`Transcoder::run_with_progress`](super::Transcoder::run_with_progress).
#[derive(Debug, Clone, Copy)]
pub struct Progress {
    pub(crate) processed_secs: f64,
    pub(crate) total_secs: f64,
    pub(crate) frames: u64,
    pub(crate) fps: f64,
}

impl Progress {
    /// Build a snapshot, deriving throughput from `frames` over the time since `started`
    /// (floored so it never divides by zero). Shared by the transcode and extraction runners.
    pub(crate) fn new(processed_secs: f64, total_secs: f64, frames: u64, started: Instant) -> Self {
        let elapsed = started.elapsed().as_secs_f64().max(1e-6);
        Self { processed_secs, total_secs, frames, fps: frames as f64 / elapsed }
    }

    /// Completion as a percentage in `[0, 100]` (best-effort; `0` if total is unknown).
    pub fn percent(&self) -> f64 {
        if self.total_secs > 0.0 {
            (self.processed_secs / self.total_secs * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        }
    }

    /// Seconds of media processed so far.
    pub fn processed_secs(&self) -> f64 {
        self.processed_secs
    }

    /// Total media duration in seconds (`0.0` if unknown).
    pub fn total_secs(&self) -> f64 {
        self.total_secs
    }

    /// Video frames encoded so far.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Encoding throughput in frames per second.
    pub fn fps(&self) -> f64 {
        self.fps
    }
}

/// The outcome of a completed transcode.
#[derive(Debug, Clone, Copy)]
pub struct TranscodeSummary {
    /// Total video frames encoded.
    pub frames: u64,
    /// Output media duration in seconds (best-effort).
    pub duration_secs: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_is_ratio_of_processed_to_total() {
        let p = Progress { processed_secs: 3.0, total_secs: 12.0, frames: 90, fps: 30.0 };
        assert_eq!(p.percent(), 25.0);
        // The accessors expose the raw fields.
        assert_eq!(p.processed_secs(), 3.0);
        assert_eq!(p.total_secs(), 12.0);
        assert_eq!(p.frames(), 90);
        assert_eq!(p.fps(), 30.0);
    }

    #[test]
    fn percent_handles_unknown_and_overshooting_totals() {
        // Unknown total → 0%, never a divide-by-zero.
        let unknown = Progress { processed_secs: 5.0, total_secs: 0.0, frames: 0, fps: 0.0 };
        assert_eq!(unknown.percent(), 0.0);

        // Processing past the estimated duration clamps to 100%.
        let overshoot = Progress { processed_secs: 20.0, total_secs: 10.0, frames: 0, fps: 0.0 };
        assert_eq!(overshoot.percent(), 100.0);
    }
}
