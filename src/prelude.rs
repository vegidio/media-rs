//! The one import most users need: `use media::prelude::*;`.
//!
//! Re-exports the common entry points and strongly-typed building blocks so they don't have
//! to be hunted down across modules.

pub use crate::error::{Error, Result};

pub use crate::codec::{Decoder, VideoEncoder};
pub use crate::extract::{
    ExtractReport, ExtractedFrame, FrameExtractor, ImageFormat, Interval, NamingScheme, Output, Resolution,
    SampledFrames, extract_frames,
};
pub use crate::filter::{ColorCorrect, DenoiseLevel, FilterChain};
pub use crate::format::{MediaReader, MediaWriter};
pub use crate::frame::Frame;
pub use crate::log::{self, Level};
pub use crate::packet::Packet;
pub use crate::probe::{MediaInfo, StreamInfo, probe};
pub use crate::transcode::{Progress, TranscodeSummary, Transcoder, VideoConfig, transcode};

pub use crate::types::{
    AudioCodec, Bitrate, Channels, Framerate, H264Preset, H264Profile, PixelFormat, Rational, SampleFormat, SampleRate,
    StreamKind, VideoCodec,
};
