//! Decoded (uncompressed) media frames.

use crate::raw::frame::RawFrame;
use crate::types::pixel_format::PixelFormat;
use crate::types::sample_format::SampleFormat;

/// A decoded frame: a single image (video) or a buffer of audio samples.
///
/// Frames yielded by a decoder own their data outright (the decoder moves its reference out
/// into a fresh frame), so you can keep, buffer, or process them freely.
pub struct Frame {
    pub(crate) raw: RawFrame,
}

impl Frame {
    pub(crate) fn from_raw(raw: RawFrame) -> Self {
        Self { raw }
    }

    /// Width in pixels (video frames; `0` for audio).
    pub fn width(&self) -> u32 {
        self.raw.width().max(0) as u32
    }

    /// Height in pixels (video frames; `0` for audio).
    pub fn height(&self) -> u32 {
        self.raw.height().max(0) as u32
    }

    /// The pixel format, for video frames.
    pub fn pixel_format(&self) -> PixelFormat {
        PixelFormat::from_av(self.raw.format())
    }

    /// The sample format, for audio frames.
    pub fn sample_format(&self) -> SampleFormat {
        SampleFormat::from_av(self.raw.format())
    }

    /// Number of audio samples per channel (audio frames).
    pub fn sample_count(&self) -> u32 {
        self.raw.nb_samples().max(0) as u32
    }

    /// Sample rate in Hz (audio frames).
    pub fn sample_rate(&self) -> u32 {
        self.raw.sample_rate().max(0) as u32
    }

    /// The presentation timestamp, in the source stream's time base
    /// (`None` if unknown).
    pub fn pts(&self) -> Option<i64> {
        let pts = self.raw.pts();
        if pts == i64::MIN { None } else { Some(pts) }
    }

    /// Set the presentation timestamp. Set this in the target encoder's time base before
    /// handing the frame to an encoder.
    pub fn set_pts(&mut self, pts: i64) {
        self.raw.set_pts(pts);
    }

    /// The decoder's best estimate of this frame's timestamp (`None` if unknown). Prefer
    /// this over [`pts`](Self::pts) when re-encoding.
    pub fn best_effort_timestamp(&self) -> Option<i64> {
        let ts = self.raw.best_effort_timestamp();
        if ts == i64::MIN { None } else { Some(ts) }
    }

    /// This frame's usable presentation timestamp in the source time base: the best-effort
    /// estimate, falling back to the raw [`pts`](Self::pts), then to `0` when both are unknown.
    pub(crate) fn best_ts(&self) -> i64 {
        self.best_effort_timestamp().or_else(|| self.pts()).unwrap_or(0)
    }
}
