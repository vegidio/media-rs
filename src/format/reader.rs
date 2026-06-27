//! Reading and demuxing media: [`MediaReader`].

use crate::codec::decoder::Decoder;
use crate::error::{Error, Result};
use crate::packet::Packet;
use crate::raw::format_context::InputFormatContext;
use crate::raw::packet::RawPacket;
use crate::types::rational::Rational;
use crate::types::stream_kind::StreamKind;

/// Opens a media file and exposes its streams and packets.
///
/// ```no_run
/// use media::prelude::*;
/// let mut reader = MediaReader::open("input.mp4")?;
/// let idx = reader.best_stream(StreamKind::Video)?;
/// let mut decoder = reader.stream(idx).decoder()?;
/// for packet in reader.packets() {
///     let packet = packet?;
///     if packet.stream_index() != idx { continue; }
///     for frame in decoder.decode(&packet)? {
///         let _frame = frame?;
///     }
/// }
/// # Ok::<(), media::Error>(())
/// ```
pub struct MediaReader {
    input: InputFormatContext,
}

impl MediaReader {
    /// Open `path` for reading and probe its stream layout.
    pub fn open(path: impl AsRef<str>) -> Result<Self> {
        Ok(Self {
            input: InputFormatContext::open(path.as_ref())?,
        })
    }

    /// The number of streams in the file.
    pub fn stream_count(&self) -> usize {
        self.input.stream_count()
    }

    /// The container's estimated duration, in seconds (`0.0` if unknown).
    pub fn duration_secs(&self) -> f64 {
        self.input.duration_secs()
    }

    /// The index of the best stream of `kind`.
    ///
    /// Errors with [`Error::NoVideoStream`]/[`Error::NoAudioStream`] when none exists.
    pub fn best_stream(&self, kind: StreamKind) -> Result<usize> {
        self.input.best_stream(kind).ok_or(match kind {
            StreamKind::Audio => Error::NoAudioStream,
            _ => Error::NoVideoStream,
        })
    }

    /// A handle to stream `index` for inspection or building a decoder.
    pub fn stream(&self, index: usize) -> StreamRef<'_> {
        StreamRef {
            reader: self,
            index,
        }
    }

    /// The kind of stream `index`.
    pub fn stream_kind(&self, index: usize) -> Result<StreamKind> {
        self.input.stream_kind(index)
    }

    /// The time base of stream `index`.
    pub fn stream_time_base(&self, index: usize) -> Result<Rational> {
        self.input.stream_time_base(index)
    }

    /// The average frame rate of stream `index` (may be `0/0`).
    pub fn stream_avg_frame_rate(&self, index: usize) -> Result<Rational> {
        self.input.stream_avg_frame_rate(index)
    }

    pub(crate) fn input(&self) -> &InputFormatContext {
        &self.input
    }

    /// Iterate over every packet in the file, in interleaved order.
    pub fn packets(&mut self) -> Packets<'_> {
        Packets { reader: self }
    }
}

/// A lightweight handle to one stream of a [`MediaReader`].
pub struct StreamRef<'r> {
    reader: &'r MediaReader,
    index: usize,
}

impl StreamRef<'_> {
    /// This stream's index.
    pub fn index(&self) -> usize {
        self.index
    }

    /// This stream's media kind.
    pub fn kind(&self) -> Result<StreamKind> {
        self.reader.input.stream_kind(self.index)
    }

    /// Build a [`Decoder`] for this stream. The decoder owns its own state and does not
    /// borrow the reader, so you can decode while iterating [`MediaReader::packets`].
    pub fn decoder(&self) -> Result<Decoder> {
        let par = self.reader.input.stream_codecpar(self.index)?;
        let codec_id = self.reader.input.stream_codec_id(self.index)?;
        Decoder::open(codec_id, par)
    }
}

/// Iterator over a [`MediaReader`]'s packets. Each item is an owned [`Packet`].
pub struct Packets<'r> {
    reader: &'r mut MediaReader,
}

impl Iterator for Packets<'_> {
    type Item = Result<Packet>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut raw = match RawPacket::alloc() {
            Ok(r) => r,
            Err(e) => return Some(Err(e)),
        };
        match self.reader.input.read_packet(&mut raw) {
            Ok(true) => Some(Ok(Packet::from_raw(raw))),
            Ok(false) => None,
            Err(e) => Some(Err(e)),
        }
    }
}
