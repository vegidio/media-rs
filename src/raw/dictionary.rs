//! RAII wrapper for `AVDictionary`: the options handed to a muxer or an encoder when it opens.

use crate::error::{Error, Result, check};
use crate::sys;
use std::ffi::{CStr, CString};
use std::ptr;

/// Set `key` to `value` in a builder's option list: a later value for the same key replaces the earlier one.
pub(crate) fn set_option(opts: &mut Vec<(String, String)>, key: &str, value: &str) {
    match opts.iter_mut().find(|(k, _)| k == key) {
        Some(entry) => entry.1 = value.to_owned(),
        None => opts.push((key.to_owned(), value.to_owned())),
    }
}

/// Convert a builder's option list to C strings, failing with `nul_msg` when a key or value contains a NUL byte.
pub(crate) fn to_c_options(opts: Vec<(String, String)>, nul_msg: &'static str) -> Result<Vec<(CString, CString)>> {
    opts.into_iter()
        .map(|(k, v)| Ok((CString::new(k)?, CString::new(v)?)))
        .collect::<std::result::Result<Vec<_>, std::ffi::NulError>>()
        .map_err(|_| Error::InvalidConfig(nul_msg))
}

/// An owned `AVDictionary`, freed on drop.
pub(crate) struct Dictionary(pub(crate) *mut sys::AVDictionary);

impl Dictionary {
    pub(crate) fn new(entries: &[(CString, CString)]) -> Result<Self> {
        let mut dict = Self(ptr::null_mut());
        for (key, value) in entries {
            // SAFETY: dict.0 is a valid dictionary slot; key and value are copied in.
            check(unsafe { sys::av_dict_set(&mut dict.0, key.as_ptr(), value.as_ptr(), 0) })?;
        }
        Ok(dict)
    }

    /// The keys still in the dictionary, in insertion order. After an open call, the options it didn't recognise.
    pub(crate) fn keys(&self) -> Vec<String> {
        let mut keys = Vec::new();
        let mut entry: *const sys::AVDictionaryEntry = ptr::null();
        loop {
            // SAFETY: an empty key with IGNORE_SUFFIX walks every entry, starting after `entry`.
            entry = unsafe { sys::av_dict_get(self.0, c"".as_ptr(), entry, sys::AV_DICT_IGNORE_SUFFIX as i32) };
            if entry.is_null() {
                return keys;
            }
            // SAFETY: a returned entry has a valid NUL-terminated key.
            keys.push(unsafe { CStr::from_ptr((*entry).key) }.to_string_lossy().into_owned());
        }
    }

    /// After an open call, fail with [`Error::UnknownOption`] naming every option it didn't recognise.
    pub(crate) fn ensure_consumed(&self) -> Result<()> {
        let unknown = self.keys();
        if unknown.is_empty() { Ok(()) } else { Err(Error::UnknownOption(unknown.join(", "))) }
    }
}

impl Drop for Dictionary {
    fn drop(&mut self) {
        // SAFETY: av_dict_free accepts a null dictionary and nulls the pointer.
        unsafe { sys::av_dict_free(&mut self.0) };
    }
}
