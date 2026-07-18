//! Small helpers shared by the RAII wrappers.

use crate::error::{Error, Result};
use std::ptr::NonNull;

/// Turn a possibly-null pointer returned by an FFmpeg allocator into a [`NonNull`], mapping
/// null to [`Error::AllocFailed`] tagged with `what`.
pub(crate) fn non_null<T>(ptr: *mut T, what: &'static str) -> Result<NonNull<T>> {
    NonNull::new(ptr).ok_or(Error::AllocFailed(what))
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
