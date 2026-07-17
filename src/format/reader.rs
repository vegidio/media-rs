//! Reading and demuxing media: [`MediaReader`].

use crate::codec::decoder::Decoder;
use crate::error::{Error, Result};
use crate::extract::Interval;
use crate::extract::sampler::SampledFrames;
use crate::packet::Packet;
use crate::raw::format_context::InputFormatContext;
use crate::raw::packet::RawPacket;
use crate::types::rational::Rational;
use crate::types::stream_kind::StreamKind;
use std::time::Duration;

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
        crate::log::ensure_init();
        Ok(Self { input: InputFormatContext::open(path.as_ref())? })
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

    /// A handle to stream `index` for inspection, building a decoder, or sampling frames.
    pub fn stream(&mut self, index: usize) -> StreamRef<'_> {
        StreamRef { reader: self, index }
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

    /// Seek so that subsequent reads resume near `at` within stream `stream_index`. Lands at
    /// or before the nearest keyframe; decode forward to reach an exact frame, and call
    /// [`Decoder::reset`](crate::codec::Decoder::reset) on any decoder you're driving so its
    /// pre-seek buffers are discarded.
    pub fn seek(&mut self, stream_index: usize, at: Duration) -> Result<()> {
        let tb = self.input.stream_time_base(stream_index)?;
        let ts = tb.ts_from_secs(at.as_secs_f64());
        self.input.seek(stream_index, ts)
    }

    /// Seek by a raw timestamp already expressed in stream `stream_index`'s time base.
    pub(crate) fn seek_ts(&mut self, stream_index: usize, ts: i64) -> Result<()> {
        self.input.seek(stream_index, ts)
    }

    /// Read the next packet, or `None` at end of input. The single-packet counterpart to
    /// [`packets`](Self::packets), used by samplers that interleave reads with seeks.
    pub(crate) fn next_packet(&mut self) -> Result<Option<Packet>> {
        let mut raw = RawPacket::alloc()?;
        if self.input.read_packet(&mut raw)? { Ok(Some(Packet::from_raw(raw))) } else { Ok(None) }
    }

    /// Iterate over every packet in the file, in interleaved order.
    pub fn packets(&mut self) -> Packets<'_> {
        Packets { reader: self }
    }
}

/// A handle to one stream of a [`MediaReader`], borrowing it for the handle's lifetime.
pub struct StreamRef<'r> {
    reader: &'r mut MediaReader,
    index: usize,
}

impl<'r> StreamRef<'r> {
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

    /// Sample this (video) stream at `interval`, yielding an iterator of decoded frames at
    /// full source resolution. The iterator seeks and decodes only what each sample point
    /// needs, so sparse sampling of a long video stays fast. Each item is an
    /// [`ExtractedFrame`](crate::extract::ExtractedFrame) carrying packed RGB pixels, its
    /// running index, and its timestamp.
    pub fn sampled_at(self, interval: Interval) -> Result<SampledFrames<'r>> {
        SampledFrames::from_stream(self.reader, self.index, interval)
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
