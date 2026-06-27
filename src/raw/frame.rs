//! RAII wrapper for `AVFrame`.

use super::util::non_null;
use crate::error::Result;
use crate::sys;
use std::ptr::NonNull;

/// An owned `AVFrame`. Freed with `av_frame_free` on drop.
pub(crate) struct RawFrame {
    ptr: NonNull<sys::AVFrame>,
}

impl RawFrame {
    /// Allocate an empty frame (no data buffers).
    pub(crate) fn alloc() -> Result<Self> {
        // SAFETY: av_frame_alloc allocates an AVFrame or returns null.
        let ptr = unsafe { sys::av_frame_alloc() };
        Ok(Self {
            ptr: non_null(ptr, "AVFrame")?,
        })
    }

    pub(crate) fn as_ptr(&self) -> *const sys::AVFrame {
        self.ptr.as_ptr()
    }

    pub(crate) fn as_mut_ptr(&mut self) -> *mut sys::AVFrame {
        self.ptr.as_ptr()
    }

    pub(crate) fn width(&self) -> i32 {
        unsafe { (*self.ptr.as_ptr()).width }
    }

    pub(crate) fn height(&self) -> i32 {
        unsafe { (*self.ptr.as_ptr()).height }
    }

    pub(crate) fn format(&self) -> i32 {
        unsafe { (*self.ptr.as_ptr()).format }
    }

    pub(crate) fn nb_samples(&self) -> i32 {
        unsafe { (*self.ptr.as_ptr()).nb_samples }
    }

    pub(crate) fn sample_rate(&self) -> i32 {
        unsafe { (*self.ptr.as_ptr()).sample_rate }
    }

    pub(crate) fn pts(&self) -> i64 {
        unsafe { (*self.ptr.as_ptr()).pts }
    }

    pub(crate) fn best_effort_timestamp(&self) -> i64 {
        unsafe { (*self.ptr.as_ptr()).best_effort_timestamp }
    }

    /// Set the presentation timestamp (used before sending a frame to an encoder).
    pub(crate) fn set_pts(&mut self, pts: i64) {
        unsafe { (*self.ptr.as_ptr()).pts = pts };
    }

    /// Create a new frame referencing the same data as `self` (a cheap refcount bump). Use
    /// when handing a frame to a consumer that takes ownership but the original must survive.
    pub(crate) fn clone_ref(&self) -> Result<RawFrame> {
        let dst = RawFrame::alloc()?;
        // SAFETY: both pointers are valid owned frames; av_frame_ref adds a new reference.
        let ret = unsafe { sys::av_frame_ref(dst.ptr.as_ptr(), self.ptr.as_ptr()) };
        crate::error::check(ret)?;
        Ok(dst)
    }

    /// Move the contents (refcounted buffers + metadata) of `self` into a brand-new frame,
    /// leaving `self` unreferenced and ready for the next receive. No pixel/sample copy.
    pub(crate) fn move_out(&mut self) -> Result<RawFrame> {
        let mut dst = RawFrame::alloc()?;
        // SAFETY: both pointers are valid owned frames; move_ref transfers ownership of the
        // buffers from self to dst and resets self.
        unsafe { sys::av_frame_move_ref(dst.as_mut_ptr(), self.as_mut_ptr()) };
        Ok(dst)
    }
}

impl Drop for RawFrame {
    fn drop(&mut self) {
        let mut ptr = self.ptr.as_ptr();
        // SAFETY: av_frame_free takes a pointer-to-pointer and nulls it; ptr is owned.
        unsafe { sys::av_frame_free(&mut ptr) };
    }
}

// SAFETY: a RawFrame uniquely owns its AVFrame with no shared interior state.
unsafe impl Send for RawFrame {}
