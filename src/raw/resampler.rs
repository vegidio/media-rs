//! RAII wrapper for an `SwrContext` (libswresample), used to convert audio between sample
//! formats, sample rates, and channel layouts.
//!
//! Mirrors [`CodecContext`](super::codec_context::CodecContext): an owned handle allocated in
//! `new`, freed in `Drop`. Conversion is frame-based via `swr_convert_frame`, which keeps its
//! own internal FIFO, so a flush call (`None` input) drains any buffered tail.

use super::frame::RawFrame;
use super::util::{impl_ffi_drop, non_null};
use crate::error::{Error, Result, check};
use crate::sys;
use crate::types::channel_layout::ChannelLayout;
use std::ptr::{self, NonNull};

/// An owned `SwrContext`, configured for one fixed (in → out) audio conversion.
pub(crate) struct ResampleContext {
    ptr: NonNull<sys::SwrContext>,
}

impl ResampleContext {
    /// Configure a converter from one audio format to another. All three axes (sample format,
    /// sample rate, channel layout) may differ; an identity conversion is a cheap pass-through.
    pub(crate) fn new(
        in_fmt: sys::AVSampleFormat,
        in_rate: i32,
        in_layout: &ChannelLayout,
        out_fmt: sys::AVSampleFormat,
        out_rate: i32,
        out_layout: &ChannelLayout,
    ) -> Result<Self> {
        let mut raw: *mut sys::SwrContext = ptr::null_mut();
        // SAFETY: raw is a valid out-param; the layout pointers are valid for the call and only
        // read; swr copies what it needs.
        check(unsafe {
            sys::swr_alloc_set_opts2(
                &mut raw,
                out_layout.as_ptr(),
                out_fmt,
                out_rate,
                in_layout.as_ptr(),
                in_fmt,
                in_rate,
                0,
                ptr::null_mut(),
            )
        })?;
        let ptr = non_null(raw, "SwrContext")?;
        let mut this = Self { ptr };
        // SAFETY: ptr is a freshly allocated, fully configured context.
        check(unsafe { sys::swr_init(this.as_mut_ptr()) })?;
        Ok(this)
    }

    fn as_mut_ptr(&mut self) -> *mut sys::SwrContext {
        self.ptr.as_ptr()
    }

    /// An upper bound on the number of output samples produced if `in_samples` are fed next
    /// (accounts for samples already buffered inside swr). Used to size the output frame.
    pub(crate) fn out_samples(&mut self, in_samples: i32) -> i32 {
        // SAFETY: ptr is valid and initialised.
        unsafe { sys::swr_get_out_samples(self.as_mut_ptr(), in_samples) }.max(0)
    }

    /// Convert `src` into `dst` (whose audio spec and buffers must already be set up for the
    /// output format). Pass `None` to flush the internal FIFO at end of stream. After the call
    /// `dst.nb_samples()` reflects how many samples were actually written.
    pub(crate) fn convert(&mut self, src: Option<&RawFrame>, dst: &mut RawFrame) -> Result<()> {
        let in_ptr = src.map_or(ptr::null(), |f| f.as_ptr());
        // SAFETY: ptr is valid; dst is a valid owned frame with output spec set; in_ptr is null
        // or a valid input frame.
        check(unsafe { sys::swr_convert_frame(self.as_mut_ptr(), dst.as_mut_ptr(), in_ptr) }).map_err(|e| match e {
            // Keep the real AVERROR code/message (the actionable diagnostic), just add context.
            Error::Internal { code, message } => {
                Error::Internal { code, message: format!("audio resample failed: {message}") }
            }
            other => other,
        })
    }
}

impl_ffi_drop!(ResampleContext, ptr, sys::swr_free);

// SAFETY: a ResampleContext uniquely owns its SwrContext with no shared interior state.
unsafe impl Send for ResampleContext {}
