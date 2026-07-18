//! Decoded (uncompressed) media frames.

use crate::raw::frame::RawFrame;
use crate::types::pixel_format::PixelFormat;
use crate::types::sample_format::SampleFormat;

/// Mutable, typed access to a decoded audio frame's raw PCM samples.
///
/// Variants mirror [`SampleFormat`]: interleaved formats expose a single slice with samples
/// packed across channels; planar formats (`…p`) expose one slice per channel. Non-audio
/// frames and formats not enumerated here yield [`Other`](Self::Other).
///
/// ```no_run
/// use media::prelude::*;
/// # fn demo(frame: &mut Frame) {
/// if let SampleBuffer::Fltp(planes) = frame.samples_mut() {
///     for plane in planes {
///         for sample in plane.iter_mut() {
///             *sample *= 0.5; // halve the gain
///         }
///     }
/// }
/// # }
/// ```
pub enum SampleBuffer<'a> {
    /// Unsigned 8-bit, interleaved.
    U8(&'a mut [u8]),
    /// Signed 16-bit, interleaved.
    S16(&'a mut [i16]),
    /// Signed 32-bit, interleaved.
    S32(&'a mut [i32]),
    /// 32-bit float, interleaved.
    Flt(&'a mut [f32]),
    /// 64-bit float, interleaved.
    Dbl(&'a mut [f64]),
    /// Signed 16-bit, planar (one slice per channel).
    S16p(Vec<&'a mut [i16]>),
    /// Signed 32-bit, planar.
    S32p(Vec<&'a mut [i32]>),
    /// 32-bit float, planar.
    Fltp(Vec<&'a mut [f32]>),
    /// 64-bit float, planar.
    Dblp(Vec<&'a mut [f64]>),
    /// A non-audio frame, or a sample format not exposed as typed slices.
    Other,
}

/// Build a typed mutable slice from a plane pointer, yielding an empty slice for a null plane.
///
/// SAFETY: `p` must be null or point to at least `count` valid `T`s for the returned lifetime,
/// and callers must not create overlapping slices (audio planes are disjoint).
unsafe fn slice_mut<'a, T>(p: *mut u8, count: usize) -> &'a mut [T] {
    let (ptr, len) = if p.is_null() {
        (std::ptr::NonNull::<T>::dangling().as_ptr(), 0)
    } else {
        (p as *mut T, count)
    };
    // SAFETY: a dangling-but-aligned pointer with len 0 is a valid empty slice; otherwise the
    // caller guarantees `count` valid, non-overlapping elements.
    unsafe { std::slice::from_raw_parts_mut(ptr, len) }
}

/// One typed mutable slice per channel plane (for planar audio).
///
/// SAFETY: same contract as [`slice_mut`], for each of the frame's `ch` planes of `nb` samples.
unsafe fn planes_mut<'a, T>(frame: &RawFrame, ch: usize, nb: usize) -> Vec<&'a mut [T]> {
    (0..ch).map(|i| unsafe { slice_mut(frame.plane_ptr(i), nb) }).collect()
}

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

    /// Mutable, typed access to this audio frame's PCM samples for custom DSP. See
    /// [`SampleBuffer`]. Non-audio frames yield [`SampleBuffer::Other`].
    pub fn samples_mut(&mut self) -> SampleBuffer<'_> {
        let nb = self.raw.nb_samples().max(0) as usize;
        let ch = self.raw.channels().max(0) as usize;
        let plane0 = self.raw.plane_ptr(0);

        // One interleaved slice of `nb * channels`, or one planar slice of `nb` per channel.
        // SAFETY: for an allocated audio frame each plane holds this many samples of the
        // reported format; planar planes are disjoint, so the per-channel &mut slices don't
        // alias. The returned buffer borrows `self` mutably for its whole lifetime.
        unsafe {
            match self.sample_format() {
                SampleFormat::U8 => SampleBuffer::U8(slice_mut(plane0, nb * ch)),
                SampleFormat::S16 => SampleBuffer::S16(slice_mut(plane0, nb * ch)),
                SampleFormat::S32 => SampleBuffer::S32(slice_mut(plane0, nb * ch)),
                SampleFormat::Flt => SampleBuffer::Flt(slice_mut(plane0, nb * ch)),
                SampleFormat::Dbl => SampleBuffer::Dbl(slice_mut(plane0, nb * ch)),
                SampleFormat::S16p => SampleBuffer::S16p(planes_mut(&self.raw, ch, nb)),
                SampleFormat::S32p => SampleBuffer::S32p(planes_mut(&self.raw, ch, nb)),
                SampleFormat::Fltp => SampleBuffer::Fltp(planes_mut(&self.raw, ch, nb)),
                SampleFormat::Dblp => SampleBuffer::Dblp(planes_mut(&self.raw, ch, nb)),
                SampleFormat::Other(_) => SampleBuffer::Other,
            }
        }
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
