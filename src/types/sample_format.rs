//! Audio sample formats, mirroring [`sys::AVSampleFormat`].

use crate::sys;

/// An audio sample format. Variants ending in `P` are *planar* (one buffer per channel);
/// the others are *interleaved* (samples packed across channels in one buffer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SampleFormat {
    /// Unsigned 8-bit, interleaved.
    U8,
    /// Signed 16-bit, interleaved.
    S16,
    /// Signed 32-bit, interleaved.
    S32,
    /// 32-bit float, interleaved.
    Flt,
    /// 64-bit float, interleaved.
    Dbl,
    /// Signed 16-bit, planar.
    S16p,
    /// Signed 32-bit, planar.
    S32p,
    /// 32-bit float, planar (the common encoder input).
    Fltp,
    /// 64-bit float, planar.
    Dblp,
    /// Any format not enumerated above; carries the raw `AVSampleFormat` value.
    Other(sys::AVSampleFormat),
}

impl SampleFormat {
    // Used by the upcoming audio re-encode path; `from_av` is already in use for decoded
    // audio frames.
    #[allow(dead_code)]
    pub(crate) fn to_av(self) -> sys::AVSampleFormat {
        match self {
            SampleFormat::U8 => sys::AVSampleFormat_AV_SAMPLE_FMT_U8,
            SampleFormat::S16 => sys::AVSampleFormat_AV_SAMPLE_FMT_S16,
            SampleFormat::S32 => sys::AVSampleFormat_AV_SAMPLE_FMT_S32,
            SampleFormat::Flt => sys::AVSampleFormat_AV_SAMPLE_FMT_FLT,
            SampleFormat::Dbl => sys::AVSampleFormat_AV_SAMPLE_FMT_DBL,
            SampleFormat::S16p => sys::AVSampleFormat_AV_SAMPLE_FMT_S16P,
            SampleFormat::S32p => sys::AVSampleFormat_AV_SAMPLE_FMT_S32P,
            SampleFormat::Fltp => sys::AVSampleFormat_AV_SAMPLE_FMT_FLTP,
            SampleFormat::Dblp => sys::AVSampleFormat_AV_SAMPLE_FMT_DBLP,
            SampleFormat::Other(v) => v,
        }
    }

    pub(crate) fn from_av(v: sys::AVSampleFormat) -> Self {
        #[allow(non_upper_case_globals)]
        match v {
            sys::AVSampleFormat_AV_SAMPLE_FMT_U8 => SampleFormat::U8,
            sys::AVSampleFormat_AV_SAMPLE_FMT_S16 => SampleFormat::S16,
            sys::AVSampleFormat_AV_SAMPLE_FMT_S32 => SampleFormat::S32,
            sys::AVSampleFormat_AV_SAMPLE_FMT_FLT => SampleFormat::Flt,
            sys::AVSampleFormat_AV_SAMPLE_FMT_DBL => SampleFormat::Dbl,
            sys::AVSampleFormat_AV_SAMPLE_FMT_S16P => SampleFormat::S16p,
            sys::AVSampleFormat_AV_SAMPLE_FMT_S32P => SampleFormat::S32p,
            sys::AVSampleFormat_AV_SAMPLE_FMT_FLTP => SampleFormat::Fltp,
            sys::AVSampleFormat_AV_SAMPLE_FMT_DBLP => SampleFormat::Dblp,
            other => SampleFormat::Other(other),
        }
    }
}
