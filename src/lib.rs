//! # media
//!
//! Rust bindings to [FFmpeg](https://ffmpeg.org) (libavcodec, libavformat, libavutil,
//! libavfilter, libavdevice, libswscale, libswresample, libpostproc) via statically
//! linked libraries.
//!
//! The static binaries are downloaded and linked at build time from
//! [`vegidio/binaries-ffmpeg`](https://github.com/vegidio/binaries-ffmpeg). The raw FFI
//! bindings are pre-generated and committed (`src/sys/bindings.rs`), included
//! unconditionally — so no build-time `bindgen`/`libclang` is needed. On top of those raw
//! [`sys`] bindings this crate provides a safe, idiomatic API; the entry points most users
//! want are re-exported from the [`prelude`].

pub mod sys;

pub mod error;
pub mod types;

mod raw;

pub mod codec;
pub mod filter;
pub mod format;
pub mod frame;
pub mod log;
pub mod packet;
pub mod prelude;
pub mod probe;
pub mod transcode;

pub use codec::{Decoder, VideoEncoder};
pub use log::Level;
pub use format::{MediaReader, MediaWriter};
pub use frame::Frame;
pub use packet::Packet;
pub use probe::{probe, MediaInfo, StreamInfo};
pub use transcode::{transcode, Transcoder, VideoConfig};

pub use error::{Error, Result};

use std::ffi::CStr;

/// Returns the FFmpeg build version string of the linked libraries, e.g. `"8.1.2"`.
///
/// This is a thin wrapper over [`sys::av_version_info`], primarily useful as a smoke
/// test that the static libraries are correctly linked and callable.
pub fn version_info() -> &'static str {
    // SAFETY: `av_version_info` returns a pointer to a static, NUL-terminated C string
    // that lives for the duration of the program.
    unsafe {
        let ptr = sys::av_version_info();
        if ptr.is_null() {
            return "";
        }
        CStr::from_ptr(ptr).to_str().unwrap_or("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_and_reports_version() {
        // Proves the static libavutil is linked and callable.
        let version = version_info();
        assert!(!version.is_empty(), "av_version_info() returned empty");

        // libavformat is in a different static archive; calling it confirms that one
        // links too.
        let fmt_version = unsafe { sys::avformat_version() };
        assert!(fmt_version > 0, "avformat_version() returned 0");
    }
}
