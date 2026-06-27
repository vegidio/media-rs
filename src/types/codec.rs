//! Strongly-typed codecs.
//!
//! Encoders are resolved **by name** (`avcodec_find_encoder_by_name`) because this is a
//! specific GPL FFmpeg build: the external libraries (`libx264`, `libx265`, …) are the ones
//! we want, not whatever native encoder shares the codec id. Decoders are resolved from a
//! stream's [`sys::AVCodecID`].

use crate::sys;

/// A video codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoCodec {
    /// H.264 / AVC (`libx264`).
    H264,
    /// H.265 / HEVC (`libx265`).
    H265,
    /// VP8 (`libvpx`).
    Vp8,
    /// VP9 (`libvpx-vp9`).
    Vp9,
    /// AV1 (`libsvtav1`).
    Av1,
}

impl VideoCodec {
    /// The encoder name to pass to `avcodec_find_encoder_by_name`.
    pub fn encoder_name(self) -> &'static str {
        match self {
            VideoCodec::H264 => "libx264",
            VideoCodec::H265 => "libx265",
            VideoCodec::Vp8 => "libvpx",
            VideoCodec::Vp9 => "libvpx-vp9",
            VideoCodec::Av1 => "libsvtav1",
        }
    }

    /// The FFmpeg codec id.
    pub fn codec_id(self) -> sys::AVCodecID {
        let id = match self {
            VideoCodec::H264 => sys::AVCodecID_AV_CODEC_ID_H264,
            VideoCodec::H265 => sys::AVCodecID_AV_CODEC_ID_HEVC,
            VideoCodec::Vp8 => sys::AVCodecID_AV_CODEC_ID_VP8,
            VideoCodec::Vp9 => sys::AVCodecID_AV_CODEC_ID_VP9,
            VideoCodec::Av1 => sys::AVCodecID_AV_CODEC_ID_AV1,
        };
        id as sys::AVCodecID
    }

    /// The best-known [`VideoCodec`] for an FFmpeg codec id, if any.
    pub(crate) fn from_codec_id(id: sys::AVCodecID) -> Option<Self> {
        #[allow(non_upper_case_globals)]
        match id {
            x if x == sys::AVCodecID_AV_CODEC_ID_H264 as sys::AVCodecID => Some(VideoCodec::H264),
            x if x == sys::AVCodecID_AV_CODEC_ID_HEVC as sys::AVCodecID => Some(VideoCodec::H265),
            x if x == sys::AVCodecID_AV_CODEC_ID_VP8 as sys::AVCodecID => Some(VideoCodec::Vp8),
            x if x == sys::AVCodecID_AV_CODEC_ID_VP9 as sys::AVCodecID => Some(VideoCodec::Vp9),
            x if x == sys::AVCodecID_AV_CODEC_ID_AV1 as sys::AVCodecID => Some(VideoCodec::Av1),
            _ => None,
        }
    }
}

/// An audio codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioCodec {
    /// AAC (native FFmpeg encoder).
    Aac,
    /// MP3 (`libmp3lame`).
    Mp3,
    /// Opus (`libopus`).
    Opus,
    /// Vorbis (`libvorbis`).
    Vorbis,
    /// FLAC (native FFmpeg encoder).
    Flac,
}

impl AudioCodec {
    /// The encoder name to pass to `avcodec_find_encoder_by_name`.
    pub fn encoder_name(self) -> &'static str {
        match self {
            AudioCodec::Aac => "aac",
            AudioCodec::Mp3 => "libmp3lame",
            AudioCodec::Opus => "libopus",
            AudioCodec::Vorbis => "libvorbis",
            AudioCodec::Flac => "flac",
        }
    }

    /// The FFmpeg codec id.
    pub fn codec_id(self) -> sys::AVCodecID {
        let id = match self {
            AudioCodec::Aac => sys::AVCodecID_AV_CODEC_ID_AAC,
            AudioCodec::Mp3 => sys::AVCodecID_AV_CODEC_ID_MP3,
            AudioCodec::Opus => sys::AVCodecID_AV_CODEC_ID_OPUS,
            AudioCodec::Vorbis => sys::AVCodecID_AV_CODEC_ID_VORBIS,
            AudioCodec::Flac => sys::AVCodecID_AV_CODEC_ID_FLAC,
        };
        id as sys::AVCodecID
    }

    /// The best-known [`AudioCodec`] for an FFmpeg codec id, if any.
    pub(crate) fn from_codec_id(id: sys::AVCodecID) -> Option<Self> {
        #[allow(non_upper_case_globals)]
        match id {
            x if x == sys::AVCodecID_AV_CODEC_ID_AAC as sys::AVCodecID => Some(AudioCodec::Aac),
            x if x == sys::AVCodecID_AV_CODEC_ID_MP3 as sys::AVCodecID => Some(AudioCodec::Mp3),
            x if x == sys::AVCodecID_AV_CODEC_ID_OPUS as sys::AVCodecID => Some(AudioCodec::Opus),
            x if x == sys::AVCodecID_AV_CODEC_ID_VORBIS as sys::AVCodecID => Some(AudioCodec::Vorbis),
            x if x == sys::AVCodecID_AV_CODEC_ID_FLAC as sys::AVCodecID => Some(AudioCodec::Flac),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_codec_id_roundtrips() {
        for c in [
            VideoCodec::H264,
            VideoCodec::H265,
            VideoCodec::Vp8,
            VideoCodec::Vp9,
            VideoCodec::Av1,
        ] {
            assert_eq!(VideoCodec::from_codec_id(c.codec_id()), Some(c));
        }
    }

    #[test]
    fn audio_codec_id_roundtrips() {
        for c in [
            AudioCodec::Aac,
            AudioCodec::Mp3,
            AudioCodec::Opus,
            AudioCodec::Vorbis,
            AudioCodec::Flac,
        ] {
            assert_eq!(AudioCodec::from_codec_id(c.codec_id()), Some(c));
        }
    }
}
