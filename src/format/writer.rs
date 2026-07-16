//! Writing and muxing media: [`MediaWriter`].

use crate::codec::encoder::VideoEncoder;
use crate::error::{Error, Result};
use crate::packet::Packet;
use crate::raw::format_context::OutputFormatContext;
use crate::types::rational::Rational;

/// Creates a media file and muxes encoded packets into it.
///
/// Add one output stream per encoder, write the header, write packets (each tagged with its
/// output stream index), then write the trailer:
///
/// ```no_run
/// use media::prelude::*;
/// # fn demo(encoder: &media::codec::VideoEncoder) -> media::Result<()> {
/// let mut writer = MediaWriter::create("output.mp4")?;
/// let stream = writer.add_stream_from_encoder(encoder)?;
/// writer.write_header()?;
/// // … for each packet: packet.set_stream_index(stream); writer.write_packet(&mut packet)? …
/// writer.write_trailer()?;
/// # Ok(()) }
/// ```
pub struct MediaWriter {
    output: OutputFormatContext,
    /// Source (encoder) time base per output stream index, for packet rescaling.
    source_tb: Vec<Rational>,
    header_written: bool,
}

impl MediaWriter {
    /// Create `path`, inferring the container format from its extension.
    pub fn create(path: impl AsRef<str>) -> Result<Self> {
        crate::log::ensure_init();
        Ok(Self {
            output: OutputFormatContext::create(path.as_ref())?,
            source_tb: Vec::new(),
            header_written: false,
        })
    }

    /// Add an output stream fed by `encoder`, returning its stream index.
    pub fn add_stream_from_encoder(&mut self, encoder: &VideoEncoder) -> Result<usize> {
        let index = self.output.add_stream()?;
        self.output.set_stream_params(index, encoder.codec_ctx())?;
        debug_assert_eq!(index, self.source_tb.len());
        self.source_tb.push(encoder.time_base());
        Ok(index)
    }

    /// Add an output stream that copies `src_index` from `reader` verbatim (stream-copy /
    /// remux, e.g. passing audio through untouched). Returns the new stream index.
    pub fn add_stream_copy(&mut self, reader: &crate::format::MediaReader, src_index: usize) -> Result<usize> {
        let par = reader.input().stream_codecpar(src_index)?;
        let index = self.output.add_stream_copy(par)?;
        debug_assert_eq!(index, self.source_tb.len());
        self.source_tb.push(reader.stream_time_base(src_index)?);
        Ok(index)
    }

    /// `true` if the container wants codec extradata in its header (so encoders feeding it
    /// should set the global-header flag — [`VideoEncoder`] does by default).
    pub fn wants_global_header(&self) -> bool {
        self.output.wants_global_header()
    }

    /// Write the container header. Must be called once, after all streams are added and
    /// before any packets.
    pub fn write_header(&mut self) -> Result<()> {
        self.output.write_header()?;
        self.header_written = true;
        Ok(())
    }

    /// Mux one packet. Its [`stream_index`](Packet::stream_index) selects the output stream;
    /// timestamps are rescaled from the encoder's time base to the (post-header) stream time
    /// base automatically.
    pub fn write_packet(&mut self, packet: &mut Packet) -> Result<()> {
        if !self.header_written {
            return Err(Error::InvalidConfig("write_header must be called before write_packet"));
        }
        let index = packet.stream_index();
        let src = *self.source_tb.get(index).ok_or(Error::StreamOutOfRange(index))?;
        let dst = self.output.stream_time_base(index)?;
        packet.rescale_ts(src, dst);
        packet.clear_pos();
        self.output.write_packet(&mut packet.raw)
    }

    /// Finalise and close the file.
    pub fn write_trailer(&mut self) -> Result<()> {
        self.output.write_trailer()
    }
}
