//! RAII wrapper for `AVPacket`.

use super::util::non_null;
use crate::error::Result;
use crate::sys;
use crate::types::rational::Rational;
use std::ptr::NonNull;

/// An owned `AVPacket`. Freed with `av_packet_free` on drop.
pub(crate) struct RawPacket {
    ptr: NonNull<sys::AVPacket>,
}

impl RawPacket {
    /// Allocate an empty packet.
    pub(crate) fn alloc() -> Result<Self> {
        // SAFETY: av_packet_alloc allocates an AVPacket or returns null.
        let ptr = unsafe { sys::av_packet_alloc() };
        Ok(Self { ptr: non_null(ptr, "AVPacket")? })
    }

    pub(crate) fn as_ptr(&self) -> *const sys::AVPacket {
        self.ptr.as_ptr()
    }

    pub(crate) fn as_mut_ptr(&mut self) -> *mut sys::AVPacket {
        self.ptr.as_ptr()
    }

    pub(crate) fn stream_index(&self) -> i32 {
        unsafe { (*self.ptr.as_ptr()).stream_index }
    }

    pub(crate) fn set_stream_index(&mut self, index: i32) {
        unsafe { (*self.ptr.as_ptr()).stream_index = index };
    }

    pub(crate) fn pts(&self) -> i64 {
        unsafe { (*self.ptr.as_ptr()).pts }
    }

    pub(crate) fn dts(&self) -> i64 {
        unsafe { (*self.ptr.as_ptr()).dts }
    }

    /// Shift pts/dts by `delta` (skipping the `AV_NOPTS_VALUE` sentinel).
    pub(crate) fn shift_timestamps(&mut self, delta: i64) {
        let p = self.ptr.as_ptr();
        unsafe {
            if (*p).pts != i64::MIN {
                (*p).pts -= delta;
            }
            if (*p).dts != i64::MIN {
                (*p).dts -= delta;
            }
        }
    }

    /// Reset the byte position so the muxer recomputes it.
    pub(crate) fn clear_pos(&mut self) {
        unsafe { (*self.ptr.as_ptr()).pos = -1 };
    }

    /// Rescale all timestamps from `src` to `dst` time base.
    pub(crate) fn rescale_ts(&mut self, src: Rational, dst: Rational) {
        // SAFETY: ptr is a valid owned packet.
        unsafe { sys::av_packet_rescale_ts(self.ptr.as_ptr(), src.to_av(), dst.to_av()) };
    }

    /// Move the contents of `self` into a new packet, leaving `self` empty. No data copy.
    pub(crate) fn move_out(&mut self) -> Result<RawPacket> {
        let mut dst = RawPacket::alloc()?;
        // SAFETY: both pointers are valid owned packets; move_ref transfers the buffers.
        unsafe { sys::av_packet_move_ref(dst.as_mut_ptr(), self.as_mut_ptr()) };
        Ok(dst)
    }
}

impl Drop for RawPacket {
    fn drop(&mut self) {
        let mut ptr = self.ptr.as_ptr();
        // SAFETY: av_packet_free takes a pointer-to-pointer and nulls it; ptr is owned.
        unsafe { sys::av_packet_free(&mut ptr) };
    }
}

// SAFETY: a RawPacket uniquely owns its AVPacket with no shared interior state.
unsafe impl Send for RawPacket {}
