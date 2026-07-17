//! RAII wrapper for `AVFrame`.

use super::util::non_null;
use crate::error::{Result, check};
use crate::sys;
use crate::types::channel_layout::ChannelLayout;
use std::os::raw::c_void;
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
        Ok(Self { ptr: non_null(ptr, "AVFrame")? })
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

    /// The number of audio channels (from the frame's channel layout).
    pub(crate) fn channels(&self) -> i32 {
        unsafe { (*self.ptr.as_ptr()).ch_layout.nb_channels }
    }

    /// Pointer to audio plane `index` (`extended_data[index]`), which handles >8 channels;
    /// for ≤8 channels it aliases `data[index]`. Returns null if out of range.
    pub(crate) fn plane_ptr(&self, index: usize) -> *mut u8 {
        // SAFETY: extended_data is a valid array of at least `channels` (planar) or 1 (packed)
        // plane pointers for an allocated audio frame.
        unsafe {
            let ed = (*self.ptr.as_ptr()).extended_data;
            if ed.is_null() { std::ptr::null_mut() } else { *ed.add(index) }
        }
    }

    /// The plane-pointer array (`extended_data`) as the `void**` that the audio FIFO's
    /// read/write functions expect.
    pub(crate) fn sample_planes_ptr(&self) -> *const *mut c_void {
        unsafe { (*self.ptr.as_ptr()).extended_data as *const *mut c_void }
    }

    /// Deep-copy this (audio) frame's channel layout.
    pub(crate) fn ch_layout_copy(&self) -> ChannelLayout {
        ChannelLayout::copy_from(unsafe { &(*self.ptr.as_ptr()).ch_layout })
    }

    /// Allocate an audio frame with the given spec and backing sample buffers, ready to be
    /// filled (e.g. by the FIFO or the resampler). `align = 0` lets FFmpeg choose.
    pub(crate) fn new_audio(
        sample_fmt: sys::AVSampleFormat,
        ch_layout: &ChannelLayout,
        sample_rate: i32,
        nb_samples: i32,
    ) -> Result<RawFrame> {
        let mut frame = RawFrame::alloc()?;
        // SAFETY: frame is a freshly allocated AVFrame; set the audio spec before requesting
        // buffers so av_frame_get_buffer knows how much to allocate.
        unsafe {
            let f = frame.as_mut_ptr();
            (*f).format = sample_fmt;
            (*f).sample_rate = sample_rate;
            (*f).nb_samples = nb_samples;
            ch_layout.copy_into(&mut (*f).ch_layout);
        }
        check(unsafe { sys::av_frame_get_buffer(frame.as_mut_ptr(), 0) })?;
        Ok(frame)
    }

    /// Override the reported sample count (after a FIFO read that returned fewer samples than
    /// the frame's allocated capacity, so the encoder sees the true length).
    pub(crate) fn set_nb_samples(&mut self, nb: i32) {
        unsafe { (*self.ptr.as_ptr()).nb_samples = nb };
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
