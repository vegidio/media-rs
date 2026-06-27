//! Video output configuration for the [`Transcoder`](super::Transcoder).

use crate::error::{Error, Result};
use crate::types::codec::VideoCodec;
use crate::types::preset::H264Preset;
use crate::types::profile::H264Profile;
use crate::types::rational::{Bitrate, Framerate};

/// How to encode the output video stream.
///
/// Anything left unset is inherited from the input (resolution, frame rate, pixel format).
#[derive(Debug, Clone)]
pub struct VideoConfig {
    pub(crate) codec: VideoCodec,
    pub(crate) resolution: Option<(u32, u32)>,
    pub(crate) bitrate: Option<Bitrate>,
    pub(crate) framerate: Option<Framerate>,
    pub(crate) preset: Option<H264Preset>,
    pub(crate) profile: Option<H264Profile>,
}

impl VideoConfig {
    /// Start configuring video output with the given codec.
    pub fn builder() -> VideoConfigBuilder {
        VideoConfigBuilder::default()
    }
}

/// Builder for [`VideoConfig`].
#[derive(Debug, Clone, Default)]
pub struct VideoConfigBuilder {
    codec: Option<VideoCodec>,
    resolution: Option<(u32, u32)>,
    bitrate: Option<Bitrate>,
    framerate: Option<Framerate>,
    preset: Option<H264Preset>,
    profile: Option<H264Profile>,
}

impl VideoConfigBuilder {
    /// The video codec (required).
    pub fn codec(mut self, codec: VideoCodec) -> Self {
        self.codec = Some(codec);
        self
    }

    /// Output resolution; if it differs from the input a scale filter is inserted.
    pub fn resolution(mut self, width: u32, height: u32) -> Self {
        self.resolution = Some((width, height));
        self
    }

    /// Target bit rate.
    pub fn bitrate(mut self, bitrate: Bitrate) -> Self {
        self.bitrate = Some(bitrate);
        self
    }

    /// Output frame rate.
    pub fn framerate(mut self, framerate: Framerate) -> Self {
        self.framerate = Some(framerate);
        self
    }

    /// Speed/quality preset (H.264/H.265).
    pub fn preset(mut self, preset: H264Preset) -> Self {
        self.preset = Some(preset);
        self
    }

    /// Codec profile (H.264/H.265).
    pub fn profile(mut self, profile: H264Profile) -> Self {
        self.profile = Some(profile);
        self
    }

    /// Validate and produce the [`VideoConfig`].
    pub fn build(self) -> Result<VideoConfig> {
        Ok(VideoConfig {
            codec: self
                .codec
                .ok_or(Error::InvalidConfig("video config requires a codec"))?,
            resolution: self.resolution,
            bitrate: self.bitrate,
            framerate: self.framerate,
            preset: self.preset,
            profile: self.profile,
        })
    }
}
