//! The one import most users need: `use media::prelude::*;`.
//!
//! Re-exports the common entry points and strongly-typed building blocks so they don't have
//! to be hunted down across modules.

pub use crate::error::{Error, Result};

pub use crate::codec::{Decoder, VideoEncoder};
pub use crate::filter::{ColorCorrect, DenoiseLevel, FilterChain};
pub use crate::format::{MediaReader, MediaWriter};
pub use crate::frame::Frame;
pub use crate::packet::Packet;
pub use crate::probe::{probe, MediaInfo, StreamInfo};
pub use crate::transcode::{
    transcode, Progress, TranscodeSummary, Transcoder, VideoConfig,
};

pub use crate::types::{
    AudioCodec, Bitrate, Channels, Framerate, H264Preset, H264Profile, PixelFormat, Rational,
    SampleFormat, SampleRate, StreamKind, VideoCodec,
};
