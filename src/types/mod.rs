//! Strongly-typed building blocks shared across the API: codecs, formats, and numeric
//! newtypes. Everything a normal user needs is also re-exported from the crate
//! [`prelude`](crate::prelude).

pub mod audio;
pub mod channel_layout;
pub mod codec;
pub mod pixel_format;
pub mod preset;
pub mod profile;
pub mod rational;
pub mod sample_format;
pub mod stream_kind;

pub use audio::SampleRate;
pub use channel_layout::Channels;
pub use codec::{AudioCodec, VideoCodec};
pub use pixel_format::PixelFormat;
pub use preset::H264Preset;
pub use profile::H264Profile;
pub use rational::{Bitrate, Framerate, Rational};
pub use sample_format::SampleFormat;
pub use stream_kind::StreamKind;
