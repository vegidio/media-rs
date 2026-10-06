//! Small helpers shared by the RAII wrappers.

use crate::error::{Error, Result, check};
use crate::sys;
use std::ffi::CStr;
use std::mem::MaybeUninit;
use std::os::raw::{c_char, c_int};
use std::ptr::{self, NonNull};

/// `AV_BPRINT_SIZE_UNLIMITED` (`(unsigned)-1`), a cast macro bindgen does not emit.
const AV_BPRINT_SIZE_UNLIMITED: u32 = u32::MAX;

/// Turn a possibly-null pointer returned by an FFmpeg allocator into a [`NonNull`], mapping
/// null to [`Error::AllocFailed`] tagged with `what`.
pub(crate) fn non_null<T>(ptr: *mut T, what: &'static str) -> Result<NonNull<T>> {
    NonNull::new(ptr).ok_or(Error::AllocFailed(what))
}

/// Run `print` against a fresh, unbounded `AVBPrint` and return the text it printed.
///
/// Owns the `av_bprint_init` / `av_bprint_finalize` pairing, so the buffer is released on every
/// path. `print` returns an FFmpeg code; a negative one is returned as the error. A buffer left
/// incomplete (an allocation failed while printing) is [`Error::AllocFailed`].
pub(crate) fn bprint_to_string(print: impl FnOnce(*mut sys::AVBPrint) -> c_int) -> Result<String> {
    let mut bp = MaybeUninit::<sys::AVBPrint>::uninit();
    // SAFETY: av_bprint_init fully initialises the struct in place.
    unsafe { sys::av_bprint_init(bp.as_mut_ptr(), 0, AV_BPRINT_SIZE_UNLIMITED) };
    let ret = print(bp.as_mut_ptr());
    // SAFETY: bp was initialised above; `len < size` is `av_bprint_is_complete`, an inline helper.
    let complete = unsafe { (*bp.as_ptr()).len < (*bp.as_ptr()).size };

    let mut out: *mut c_char = ptr::null_mut();
    // SAFETY: finalize frees bp's buffer, handing back a malloc'd copy in `out` (null on ENOMEM).
    let finalized = unsafe { sys::av_bprint_finalize(bp.as_mut_ptr(), &mut out) };
    let text = (!out.is_null()).then(|| {
        // SAFETY: `out` is a NUL-terminated string we now own and free right after copying.
        let text = unsafe { CStr::from_ptr(out) }.to_string_lossy().into_owned();
        unsafe { sys::av_free(out.cast()) };
        text
    });

    check(ret)?;
    check(finalized)?;
    match text {
        Some(text) if complete => Ok(text),
        _ => Err(Error::AllocFailed("AVBPrint")),
    }
}

/// Implement `Drop` for an RAII wrapper whose single owned FFmpeg handle lives in the `NonNull`
/// field `$field`, released by an FFmpeg `*_free`-style function that takes a pointer-to-pointer
/// and nulls it. Centralises the identical "copy the pointer to a local, hand the free fn its
/// address" dance the raw wrappers would otherwise each repeat.
macro_rules! impl_ffi_drop {
    ($ty:ty, $field:ident, $free:path) => {
        impl Drop for $ty {
            fn drop(&mut self) {
                let mut ptr = self.$field.as_ptr();
                // SAFETY: the free fn takes a pointer-to-pointer and nulls it; the handle is owned.
                unsafe { $free(&mut ptr) };
            }
        }
    };
}

pub(crate) use impl_ffi_drop;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::channel_layout::ChannelLayout;

    #[test]
    fn bprint_to_string_round_trips_printed_text() {
        let layout = ChannelLayout::default_for(2);
        // SAFETY: the layout is initialised and outlives the call.
        let text = bprint_to_string(|bp| unsafe { sys::av_channel_layout_describe_bprint(layout.as_ptr(), bp) });
        assert_eq!(text.unwrap(), "stereo");
    }

    #[test]
    fn bprint_to_string_returns_the_print_error_and_still_finalizes() {
        // Long enough to move the text out of AVBPrint's inline buffer onto the heap, which finalize must free.
        let long = std::ffi::CString::new("x".repeat(4096)).unwrap();
        let result = bprint_to_string(|bp| {
            // SAFETY: bp is a live AVBPrint; "%s" with one NUL-terminated string argument.
            unsafe { sys::av_bprintf(bp, c"%s".as_ptr(), long.as_ptr()) };
            -22
        });
        assert!(matches!(result, Err(Error::Internal { code: -22, .. })), "{result:?}");
    }
}
