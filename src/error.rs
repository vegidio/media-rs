//! Error types for the safe API.
//!
//! FFmpeg reports failures as negative `int` return codes. The internal `check` helper turns
//! those into a [`Result`], decoding the human-readable message with `av_strerror`. A handful
//! of values that FFmpeg only exposes as C *macros* (so bindgen never emits them) are
//! re-derived here: `AVERROR_EOF`, `EAGAIN` (per-OS) and `AVERROR_EAGAIN`.

use crate::sys;
use std::ffi::CStr;
use std::os::raw::c_char;

/// The crate-wide result type.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors returned by the safe API.
///
/// Most failures from FFmpeg surface as [`Error::Internal`], carrying the raw code and the
/// message produced by `av_strerror`. The other variants are raised by this crate when it
/// can give the caller something more actionable than a numeric code.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The requested codec is not present in this FFmpeg build (e.g. a hardware encoder on
    /// a platform that doesn't ship it). The string is the encoder/decoder name we looked
    /// up.
    #[error("codec '{0}' is not available in this FFmpeg build")]
    CodecUnavailable(String),

    /// The input has no video stream but the operation requires one.
    #[error("input has no video stream")]
    NoVideoStream,

    /// The input has no audio stream but the operation requires one.
    #[error("input has no audio stream")]
    NoAudioStream,

    /// The requested stream index does not exist in the input.
    #[error("stream index {0} is out of range")]
    StreamOutOfRange(usize),

    /// A resolution the chosen codec cannot encode.
    #[error("resolution {width}x{height} is not supported")]
    UnsupportedResolution {
        /// Requested width in pixels.
        width: i32,
        /// Requested height in pixels.
        height: i32,
    },

    /// The input could not be opened (missing file, unknown format, permissions, …).
    #[error("could not open input '{0}'")]
    OpenInput(String),

    /// The output could not be created.
    #[error("could not create output '{0}'")]
    CreateOutput(String),

    /// A heap allocation inside FFmpeg returned null.
    #[error("allocation failed for {0}")]
    AllocFailed(&'static str),

    /// The builder/config was used incorrectly (missing required field, etc.).
    #[error("invalid configuration: {0}")]
    InvalidConfig(&'static str),

    /// A path contained an interior NUL byte and could not be passed to FFmpeg.
    #[error("path contains an interior NUL byte")]
    InvalidPath,

    /// Encoding a decoded frame to an image (JPEG/PNG/…) or writing it out failed.
    #[error("image encoding failed: {0}")]
    ImageEncode(String),

    /// A raw FFmpeg error code plus its decoded message.
    #[error("ffmpeg error {code}: {message}")]
    Internal {
        /// The negative `AVERROR` code FFmpeg returned.
        code: i32,
        /// The message decoded via `av_strerror`.
        message: String,
    },
}

impl Error {
    /// Build an [`Error::Internal`] from a raw FFmpeg return code, decoding its message.
    pub(crate) fn from_code(code: i32) -> Self {
        Error::Internal { code, message: strerror(code) }
    }
}

/// Decode an FFmpeg error code into a human-readable string via `av_strerror`.
pub(crate) fn strerror(code: i32) -> String {
    // AV_ERROR_MAX_STRING_SIZE is 64; 256 leaves generous headroom. Use `c_char` (not a
    // fixed `i8`) because C `char` is unsigned on some targets (e.g. aarch64 Linux) and
    // signed on others (x86_64, Apple arm64), which changes `av_strerror`'s pointer type.
    let mut buf = [0 as c_char; 256];
    // SAFETY: `buf` is a valid, writable buffer of `buf.len()` bytes; av_strerror NUL-
    // terminates within it.
    unsafe {
        sys::av_strerror(code, buf.as_mut_ptr(), buf.len());
    }
    // SAFETY: the buffer is NUL-terminated (we zeroed it and av_strerror writes a C string).
    let bytes = unsafe { CStr::from_ptr(buf.as_ptr()) };
    bytes.to_string_lossy().into_owned()
}

/// Convert an FFmpeg return code into a `Result`: `code < 0` is an error, `>= 0` is `Ok`.
pub(crate) fn check(code: i32) -> Result<()> {
    if code < 0 { Err(Error::from_code(code)) } else { Ok(()) }
}

// --- Constants FFmpeg only defines as C macros (bindgen does not emit these) ----------

/// `AVERROR(e)` wraps a positive `errno` value into FFmpeg's negative error space.
const fn averror(e: i32) -> i32 {
    -e
}

/// `MKTAG(a, b, c, d)` — packs four bytes into a little-endian tag, matching FFmpeg.
const fn mktag(a: u8, b: u8, c: u8, d: u8) -> i32 {
    ((a as u32) | ((b as u32) << 8) | ((c as u32) << 16) | ((d as u32) << 24)) as i32
}

/// `FFERRTAG(a, b, c, d)` — the negated tag form FFmpeg uses for sentinel error codes.
const fn fferrtag(a: u8, b: u8, c: u8, d: u8) -> i32 {
    -mktag(a, b, c, d)
}

/// End of file / end of stream — returned by `receive_frame`/`receive_packet` once a codec
/// is fully drained. Equivalent to FFmpeg's `AVERROR_EOF`.
pub(crate) const AVERROR_EOF: i32 = fferrtag(b'E', b'O', b'F', b' ');

/// `EAGAIN` (resource temporarily unavailable). FFmpeg returns `AVERROR(EAGAIN)` to mean
/// "needs more input"; the numeric value of `EAGAIN` differs by platform. macOS/BSD use the
/// outlier `35`; Linux and the MSVC/UCRT C runtime both use `11`.
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub(crate) const EAGAIN: i32 = 35;
#[cfg(target_os = "linux")]
pub(crate) const EAGAIN: i32 = 11;
#[cfg(windows)]
pub(crate) const EAGAIN: i32 = 11;
// Fallback for any other target so the crate still compiles.
#[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "linux", windows)))]
pub(crate) const EAGAIN: i32 = 11;

/// `AVERROR(EAGAIN)` — the "needs more input" sentinel from the send/receive APIs.
pub(crate) const AVERROR_EAGAIN: i32 = averror(EAGAIN);

/// `AV_NOPTS_VALUE` — the "timestamp unknown" sentinel (`INT64_C(0x8000000000000000)`).
pub(crate) const AV_NOPTS_VALUE: i64 = i64::MIN;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn averror_eof_matches_ffmpeg() {
        // FFERRTAG('E','O','F',' ') == -('E' | 'O'<<8 | 'F'<<16 | ' '<<24).
        assert_eq!(AVERROR_EOF, -541_478_725);
    }

    #[test]
    fn averror_eagain_is_negative_errno() {
        assert_eq!(AVERROR_EAGAIN, -EAGAIN);
    }

    #[test]
    fn check_maps_sign() {
        assert!(check(0).is_ok());
        assert!(check(1).is_ok());
        assert!(check(-1).is_err());
    }
}
