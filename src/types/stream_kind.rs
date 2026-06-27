//! Stream kinds (video, audio, …), mirroring [`sys::AVMediaType`].

use crate::sys;

/// The kind of data carried by a media stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StreamKind {
    /// Video.
    Video,
    /// Audio.
    Audio,
    /// Subtitles.
    Subtitle,
    /// Opaque data.
    Data,
    /// Any media type not represented above (attachments, unknown, …).
    Other,
}

impl StreamKind {
    pub(crate) fn to_av(self) -> sys::AVMediaType {
        match self {
            StreamKind::Video => sys::AVMediaType_AVMEDIA_TYPE_VIDEO,
            StreamKind::Audio => sys::AVMediaType_AVMEDIA_TYPE_AUDIO,
            StreamKind::Subtitle => sys::AVMediaType_AVMEDIA_TYPE_SUBTITLE,
            StreamKind::Data => sys::AVMediaType_AVMEDIA_TYPE_DATA,
            StreamKind::Other => sys::AVMediaType_AVMEDIA_TYPE_UNKNOWN,
        }
    }

    pub(crate) fn from_av(t: sys::AVMediaType) -> Self {
        #[allow(non_upper_case_globals)]
        match t {
            sys::AVMediaType_AVMEDIA_TYPE_VIDEO => StreamKind::Video,
            sys::AVMediaType_AVMEDIA_TYPE_AUDIO => StreamKind::Audio,
            sys::AVMediaType_AVMEDIA_TYPE_SUBTITLE => StreamKind::Subtitle,
            sys::AVMediaType_AVMEDIA_TYPE_DATA => StreamKind::Data,
            _ => StreamKind::Other,
        }
    }
}
