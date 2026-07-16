//! RAII wrapper for `AVFrame`.

use super::util::non_null;
use crate::error::{Result, check};
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

    /// Copy this frame's pixels into a freshly allocated, tightly packed buffer for the given
    /// pixel format (`align = 1`, so rows have no padding — exactly what image encoders and
    /// `image::RgbImage::from_raw` expect). The frame's own storage is untouched.
    pub(crate) fn copy_to_packed_buffer(&self, pix_fmt: sys::AVPixelFormat) -> Result<Vec<u8>> {
        let (w, h) = (self.width(), self.height());
        // SAFETY: a pure size computation over valid dimensions/format.
        let size = unsafe { sys::av_image_get_buffer_size(pix_fmt, w, h, 1) };
        check(size)?;
        let mut buf = vec![0u8; size as usize];
        // SAFETY: `buf` is `size` bytes; the frame's `data`/`linesize` arrays describe valid
        // planes for `pix_fmt` at `w`×`h`; align 1 packs rows contiguously.
        let ret = unsafe {
            sys::av_image_copy_to_buffer(
                buf.as_mut_ptr(),
                size,
                (*self.ptr.as_ptr()).data.as_ptr() as *const *const u8,
                (*self.ptr.as_ptr()).linesize.as_ptr(),
                pix_fmt,
                w,
                h,
                1,
            )
        };
        check(ret)?;
        Ok(buf)
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
