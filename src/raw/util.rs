//! Small helpers shared by the RAII wrappers.

use crate::error::{Error, Result};
use std::ptr::NonNull;

/// Turn a possibly-null pointer returned by an FFmpeg allocator into a [`NonNull`], mapping
/// null to [`Error::AllocFailed`] tagged with `what`.
pub(crate) fn non_null<T>(ptr: *mut T, what: &'static str) -> Result<NonNull<T>> {
    NonNull::new(ptr).ok_or(Error::AllocFailed(what))
}
