//! Quick inspection of a media file without decoding: [`probe`].

use crate::error::Result;
use crate::format::reader::MediaReader;
use crate::types::codec::{AudioCodec, VideoCodec};
use crate::types::rational::Framerate;
use crate::types::stream_kind::StreamKind;
use std::time::Duration;

/// Inspect `path` and return its container/stream metadata. Does not decode any frames.
pub fn probe(path: impl AsRef<str>) -> Result<MediaInfo> {
    let reader = MediaReader::open(path.as_ref())?;
    let count = reader.stream_count();
    let mut streams = Vec::with_capacity(count);
    for index in 0..count {
        let kind = reader.stream_kind(index)?;
        let (width, height) = reader.input().stream_dimensions(index)?;
        let sample_rate = reader.input().stream_sample_rate(index)?;
        let codec_id = reader.input().stream_codec_id(index)?;
        let codec_name = reader.input().stream_codec_name(index)?;
        // The average, not `r_frame_rate`: that is the timebase-derived base rate, which for a variable-rate phone
        // video often reads 60 or 90000/1 where players show the average.
        let avg = reader.input().stream_avg_frame_rate(index)?;
        let frame_rate = if kind == StreamKind::Video { Framerate::known(avg) } else { None };
        let codec_string = reader.input().stream_codec_string(index, frame_rate)?;
        streams.push(StreamInfo {
            index,
            kind,
            width: width.max(0) as u32,
            height: height.max(0) as u32,
            sample_rate: sample_rate.max(0) as u32,
            video_codec: VideoCodec::from_codec_id(codec_id),
            audio_codec: AudioCodec::from_codec_id(codec_id),
            codec_name,
            codec_string,
            frame_rate,
        });
    }
    Ok(MediaInfo {
        format_name: reader.input().format_name(),
        duration: Duration::from_secs_f64(reader.duration_secs().max(0.0)),
        streams,
    })
}

/// Container-level metadata returned by [`probe`].
#[derive(Debug, Clone)]
pub struct MediaInfo {
    format_name: String,
    duration: Duration,
    streams: Vec<StreamInfo>,
}

impl MediaInfo {
    /// The demuxer's name, as FFmpeg gives it: a comma-separated list when one demuxer serves several containers,
    /// such as `mov,mp4,m4a,3gp,3g2,mj2` or `matroska,webm`. It can't tell MP4 from MOV, or WebM from Matroska.
    pub fn format_name(&self) -> &str {
        &self.format_name
    }

    /// The container's estimated duration.
    pub fn duration(&self) -> Duration {
        self.duration
    }

    /// The number of streams.
    pub fn stream_count(&self) -> usize {
        self.streams.len()
    }

    /// All streams.
    pub fn streams(&self) -> &[StreamInfo] {
        &self.streams
    }

    /// The first video stream, if any.
    pub fn video(&self) -> Option<&StreamInfo> {
        self.streams.iter().find(|s| s.kind == StreamKind::Video)
    }

    /// The first audio stream, if any.
    pub fn audio(&self) -> Option<&StreamInfo> {
        self.streams.iter().find(|s| s.kind == StreamKind::Audio)
    }
}

/// Per-stream metadata.
#[derive(Debug, Clone)]
pub struct StreamInfo {
    /// The stream's index within the container.
    pub index: usize,
    /// The stream's media kind.
    pub kind: StreamKind,
    /// Width in pixels (video; `0` otherwise).
    pub width: u32,
    /// Height in pixels (video; `0` otherwise).
    pub height: u32,
    /// Sample rate in Hz (audio; `0` otherwise).
    pub sample_rate: u32,
    /// The recognised video codec, if this is a video stream of a known type.
    pub video_codec: Option<VideoCodec>,
    /// The recognised audio codec, if this is an audio stream of a known type.
    pub audio_codec: Option<AudioCodec>,
    /// FFmpeg's name for the stream's codec, for any codec, not only the recognised ones: `h264`, `aac`, `mpeg4`,
    /// `wmv3`, `pcm_s16le`, …
    pub codec_name: String,
    /// The RFC 6381 codec string FFmpeg builds for the stream, as a `codecs=` parameter of a MIME type wants it:
    /// `avc1.640028`, `hvc1.1.6.L120.90`, `vp09.00.40.08`, `av01.0.08M.08`, `mp4a.40.2`, … The strings are FFmpeg's:
    /// some are short WebM names (`vp8`, `opus`, `flac`), and `mp4v.20` for MPEG-4 Part 2 lacks its profile and
    /// level. `None` when FFmpeg has none for the codec (WMV, ProRes, PCM, …) or the stream's parameters are too
    /// incomplete to build one (H.264 without `avcC` extradata, HEVC tagged `hev1`).
    pub codec_string: Option<String>,
    /// The average frame rate the container declares (video; `None` otherwise, or when the container declares
    /// none).
    pub frame_rate: Option<Framerate>,
}
