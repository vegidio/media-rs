//! Output stream configuration for the [`Transcoder`](super::Transcoder).

use crate::error::{Error, Result};
use crate::types::audio::SampleRate;
use crate::types::channel_layout::Channels;
use crate::types::codec::{AudioCodec, VideoCodec};
use crate::types::preset::H264Preset;
use crate::types::profile::H264Profile;
use crate::types::rational::{Bitrate, Framerate};
use crate::types::sample_format::SampleFormat;

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
            codec: self.codec.ok_or(Error::InvalidConfig("video config requires a codec"))?,
            resolution: self.resolution,
            bitrate: self.bitrate,
            framerate: self.framerate,
            preset: self.preset,
            profile: self.profile,
        })
    }
}

/// How to encode the output audio stream.
///
/// Anything left unset is inherited from the input (sample rate, channel layout) or the output
/// container (codec). The sample format defaults to whatever the chosen encoder prefers.
#[derive(Debug, Clone)]
pub struct AudioConfig {
    pub(crate) codec: AudioCodec,
    pub(crate) bitrate: Option<Bitrate>,
    pub(crate) sample_rate: Option<SampleRate>,
    pub(crate) channels: Option<Channels>,
    pub(crate) sample_format: Option<SampleFormat>,
}

impl AudioConfig {
    /// Start configuring audio output with the given codec.
    pub fn builder() -> AudioConfigBuilder {
        AudioConfigBuilder::default()
    }
}

/// Builder for [`AudioConfig`].
#[derive(Debug, Clone, Default)]
pub struct AudioConfigBuilder {
    codec: Option<AudioCodec>,
    bitrate: Option<Bitrate>,
    sample_rate: Option<SampleRate>,
    channels: Option<Channels>,
    sample_format: Option<SampleFormat>,
}

impl AudioConfigBuilder {
    /// The audio codec (required).
    pub fn codec(mut self, codec: AudioCodec) -> Self {
        self.codec = Some(codec);
        self
    }

    /// Target bit rate (ignored by lossless codecs like FLAC).
    pub fn bitrate(mut self, bitrate: Bitrate) -> Self {
        self.bitrate = Some(bitrate);
        self
    }

    /// Output sample rate (defaults to the input's).
    pub fn sample_rate(mut self, sample_rate: SampleRate) -> Self {
        self.sample_rate = Some(sample_rate);
        self
    }

    /// Output channel configuration (defaults to the input's).
    pub fn channels(mut self, channels: Channels) -> Self {
        self.channels = Some(channels);
        self
    }

    /// Sample format (defaults to the encoder's preferred format).
    pub fn sample_format(mut self, sample_format: SampleFormat) -> Self {
        self.sample_format = Some(sample_format);
        self
    }

    /// Validate and produce the [`AudioConfig`].
    pub fn build(self) -> Result<AudioConfig> {
        Ok(AudioConfig {
            codec: self.codec.ok_or(Error::InvalidConfig("audio config requires a codec"))?,
            bitrate: self.bitrate,
            sample_rate: self.sample_rate,
            channels: self.channels,
            sample_format: self.sample_format,
        })
    }
}
