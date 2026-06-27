//! Container I/O: [`MediaReader`] for demuxing and [`MediaWriter`] for muxing.

pub mod reader;
pub mod writer;

pub use reader::{MediaReader, Packets, StreamRef};
pub use writer::MediaWriter;
