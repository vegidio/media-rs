//! Quick inspection of a media file without decoding: [`probe`].

use crate::error::Result;
use crate::format::reader::MediaReader;
use crate::types::codec::{AudioCodec, VideoCodec};
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
        streams.push(StreamInfo {
            index,
            kind,
            width: width.max(0) as u32,
            height: height.max(0) as u32,
            sample_rate: sample_rate.max(0) as u32,
            video_codec: VideoCodec::from_codec_id(codec_id),
            audio_codec: AudioCodec::from_codec_id(codec_id),
        });
    }
    Ok(MediaInfo { duration: Duration::from_secs_f64(reader.duration_secs().max(0.0)), streams })
}

/// Container-level metadata returned by [`probe`].
#[derive(Debug, Clone)]
pub struct MediaInfo {
    duration: Duration,
    streams: Vec<StreamInfo>,
}

impl MediaInfo {
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
}
