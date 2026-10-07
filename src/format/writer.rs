//! Writing and muxing media: [`MediaWriter`].

use crate::codec::encoder::Encoder;
use crate::error::{Error, Result};
use crate::packet::Packet;
use crate::raw::dictionary::{set_option, to_c_options};
use crate::raw::format_context::{OutputFormatContext, OutputTarget};
use crate::types::rational::Rational;
use std::ffi::CString;
use std::io::Write;

/// The `movflags` [`MediaWriterBuilder::fragmented_mp4`] sets: cut a fragment at each keyframe, write an empty
/// `moov` up front, make sample offsets relative to each `moof`, which is what Media Source Extensions want, and
/// skip the `mfra` index at the end, which MSE doesn't use and FFmpeg would otherwise grow for every fragment.
const FRAGMENTED_MP4_FLAGS: &str = "frag_keyframe+empty_moov+default_base_moof+skip_trailer";

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
///
/// To write to something other than a file, or to pass muxer options, use [`MediaWriter::builder`].
pub struct MediaWriter {
    output: OutputFormatContext,
    /// Source (encoder) time base per output stream index, for packet rescaling.
    source_tb: Vec<Rational>,
    /// Muxer options, applied at `write_header`, which is when FFmpeg reads them.
    options: Vec<(CString, CString)>,
    header_written: bool,
}

impl MediaWriter {
    /// Create `path`, inferring the container format from its extension. The same as
    /// `MediaWriter::builder().path(path).build()`.
    pub fn create(path: impl AsRef<str>) -> Result<Self> {
        Self::builder().path(path).build()
    }

    /// Start building a writer that writes to a path or to any [`Write`], with muxer options:
    ///
    /// ```no_run
    /// use media::prelude::*;
    /// # fn demo(sink: std::fs::File) -> media::Result<()> {
    /// let mut writer = MediaWriter::builder()
    ///     .writer(sink)
    ///     .fragmented_mp4()
    ///     .option("frag_duration", "2000000")
    ///     .build()?;
    /// # Ok(()) }
    /// ```
    pub fn builder() -> MediaWriterBuilder {
        MediaWriterBuilder::default()
    }

    /// Add an output stream fed by `encoder` (a [`VideoEncoder`](crate::codec::VideoEncoder) or
    /// [`AudioEncoder`](crate::codec::AudioEncoder)), returning its stream index.
    pub fn add_stream_from_encoder<E: Encoder>(&mut self, encoder: &E) -> Result<usize> {
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
    /// should set the global-header flag — [`VideoEncoder`](crate::codec::VideoEncoder) does by
    /// default).
    pub fn wants_global_header(&self) -> bool {
        self.output.wants_global_header()
    }

    /// `true` if this container accepts the given codec id (for the stream-copy vs re-encode
    /// decision, and the video-container guard).
    pub(crate) fn supports_codec(&self, codec_id: crate::sys::AVCodecID) -> bool {
        self.output.supports_codec(codec_id)
    }

    /// The container's default audio codec id (for auto-encoding audio that can't be copied).
    pub(crate) fn default_audio_codec_id(&self) -> crate::sys::AVCodecID {
        self.output.default_audio_codec_id()
    }

    /// Write the container header. Must be called once, after all streams are added and
    /// before any packets.
    ///
    /// The builder's muxer options are applied here. Any the muxer didn't recognise fail with
    /// [`Error::UnknownOption`] before anything is written.
    pub fn write_header(&mut self) -> Result<()> {
        self.output.write_header(&self.options)?;
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

    /// Write out everything muxed so far, so a reader of the file or writer sees it now rather than when the buffer
    /// fills. With [`fragmented_mp4`](MediaWriterBuilder::fragmented_mp4) it also closes the pending fragment, so
    /// after each call the output holds only whole fragments: call it before each keyframe to hand a player one
    /// fragment at a time. A muxer that can't cut on demand just writes out what it has buffered.
    ///
    /// Valid only after [`write_header`](Self::write_header).
    pub fn flush(&mut self) -> Result<()> {
        if !self.header_written {
            return Err(Error::InvalidConfig("write_header must be called before flush"));
        }
        self.output.flush()
    }

    /// Finalise and close the file.
    pub fn write_trailer(&mut self) -> Result<()> {
        self.output.write_trailer()
    }
}

/// Builds a [`MediaWriter`]: where it writes, in which container, and with which muxer options.
///
/// Give exactly one of [`path`](Self::path) or [`writer`](Self::writer). A writer has no file extension to guess the
/// container from, so it also needs [`format`](Self::format) (or [`fragmented_mp4`](Self::fragmented_mp4)).
#[derive(Default)]
pub struct MediaWriterBuilder {
    path: Option<String>,
    writer: Option<Box<dyn Write + Send>>,
    format: Option<String>,
    options: Vec<(String, String)>,
    /// Set by `fragmented_mp4`; its flags are merged into `movflags` at `build`, whatever the call order.
    fragmented: bool,
}

impl MediaWriterBuilder {
    /// Write to the file at `path`. The container comes from its extension unless [`format`](Self::format) names one.
    pub fn path(mut self, path: impl AsRef<str>) -> Self {
        self.path = Some(path.as_ref().to_owned());
        self
    }

    /// Write to `writer` instead of a file. It needs [`format`](Self::format), and is never seeked, so the container
    /// must be one that can be written front to back, such as fragmented MP4, Matroska or MPEG-TS.
    ///
    /// The writer is `'static` because the [`MediaWriter`] owns it. To get the bytes back, pass something you share
    /// with it, such as an `Arc<Mutex<Vec<u8>>>` wrapper or a channel-backed writer.
    pub fn writer(mut self, writer: impl Write + Send + 'static) -> Self {
        self.writer = Some(Box::new(writer));
        self
    }

    /// The container to write, by FFmpeg's muxer name: `mp4`, `matroska`, `webm`, `mpegts`, … Required with
    /// [`writer`](Self::writer); with [`path`](Self::path) it overrides the extension.
    pub fn format(mut self, format: impl AsRef<str>) -> Self {
        self.format = Some(format.as_ref().to_owned());
        self
    }

    /// Pass a muxer option, such as `movflags` or `frag_duration`. Repeatable; a later value for the same key wins.
    /// An option the muxer doesn't recognise fails [`MediaWriter::write_header`] with [`Error::UnknownOption`].
    pub fn option(mut self, key: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        set_option(&mut self.options, key.as_ref(), value.as_ref());
        self
    }

    /// Write fragmented MP4 for Media Source Extensions: the `mp4` container, starting with an init segment
    /// (`ftyp` + `moov`), then `moof` + `mdat` fragments that each start on a keyframe. Other options are kept, and
    /// its flags are added to any `movflags` set with [`option`](Self::option), before or after this call.
    pub fn fragmented_mp4(mut self) -> Self {
        self.format = Some("mp4".to_owned());
        self.fragmented = true;
        self
    }

    /// Create the writer: open the file, or wrap the writer, and allocate the muxer.
    pub fn build(mut self) -> Result<MediaWriter> {
        crate::log::ensure_init();
        if self.fragmented {
            let flags = match self.options.iter().find(|(k, _)| k == "movflags") {
                Some((_, existing)) => format!("{existing}+{FRAGMENTED_MP4_FLAGS}"),
                None => FRAGMENTED_MP4_FLAGS.to_owned(),
            };
            self = self.option("movflags", flags);
        }
        let target = match (self.path, self.writer) {
            (Some(path), None) => OutputTarget::Path(path),
            (None, Some(writer)) => {
                if self.format.is_none() {
                    return Err(Error::InvalidConfig("a media writer writing to a writer requires a format"));
                }
                OutputTarget::Writer(writer)
            }
            _ => return Err(Error::InvalidConfig("a media writer requires exactly one of path or writer")),
        };
        let options = to_c_options(self.options, "a muxer option contains a NUL byte")?;
        Ok(MediaWriter {
            output: OutputFormatContext::create(target, self.format.as_deref())?,
            source_tb: Vec::new(),
            options,
            header_written: false,
        })
    }
}
