//! Frame-level codecs: [`Decoder`] and [`VideoEncoder`].

pub mod decoder;
pub mod encoder;

pub use decoder::{DecodeIter, Decoder};
pub use encoder::{EncodeIter, VideoEncoder, VideoEncoderBuilder};
