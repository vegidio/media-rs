//! Pixel formats, mirroring a useful subset of [`sys::AVPixelFormat`].

use crate::sys;

/// A pixel (image sample) format.
///
/// This is a curated subset of FFmpeg's large `AVPixelFormat` enum. Formats coming back
/// from a decoder that aren't represented here are reported as [`PixelFormat::Other`],
/// carrying the raw value so it can still be round-tripped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    /// Planar YUV 4:2:0, 12bpp (the common decode/encode format).
    Yuv420p,
    /// Planar YUV 4:2:2.
    Yuv422p,
    /// Planar YUV 4:4:4.
    Yuv444p,
    /// Semi-planar YUV 4:2:0 (Y plane + interleaved UV) — common for hardware.
    Nv12,
    /// Packed RGB 8:8:8.
    Rgb24,
    /// Packed RGBA 8:8:8:8.
    Rgba,
    /// Any format not enumerated above; carries the raw `AVPixelFormat` value.
    Other(sys::AVPixelFormat),
}

impl PixelFormat {
    pub(crate) fn to_av(self) -> sys::AVPixelFormat {
        match self {
            PixelFormat::Yuv420p => sys::AVPixelFormat_AV_PIX_FMT_YUV420P,
            PixelFormat::Yuv422p => sys::AVPixelFormat_AV_PIX_FMT_YUV422P,
            PixelFormat::Yuv444p => sys::AVPixelFormat_AV_PIX_FMT_YUV444P,
            PixelFormat::Nv12 => sys::AVPixelFormat_AV_PIX_FMT_NV12,
            PixelFormat::Rgb24 => sys::AVPixelFormat_AV_PIX_FMT_RGB24,
            PixelFormat::Rgba => sys::AVPixelFormat_AV_PIX_FMT_RGBA,
            PixelFormat::Other(v) => v,
        }
    }

    pub(crate) fn from_av(v: sys::AVPixelFormat) -> Self {
        #[allow(non_upper_case_globals)]
        match v {
            sys::AVPixelFormat_AV_PIX_FMT_YUV420P => PixelFormat::Yuv420p,
            sys::AVPixelFormat_AV_PIX_FMT_YUV422P => PixelFormat::Yuv422p,
            sys::AVPixelFormat_AV_PIX_FMT_YUV444P => PixelFormat::Yuv444p,
            sys::AVPixelFormat_AV_PIX_FMT_NV12 => PixelFormat::Nv12,
            sys::AVPixelFormat_AV_PIX_FMT_RGB24 => PixelFormat::Rgb24,
            sys::AVPixelFormat_AV_PIX_FMT_RGBA => PixelFormat::Rgba,
            other => PixelFormat::Other(other),
        }
    }
}
