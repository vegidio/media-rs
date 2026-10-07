//! RAII wrappers for input (demuxer) and output (muxer) `AVFormatContext`s.
//!
//! The two have **different** teardown paths: an input context is released with
//! `avformat_close_input`, while an output context needs its AVIO closed (when the format
//! writes to a file) and then `avformat_free_context`.

use super::codec_context::CodecContext;
use super::dictionary::Dictionary;
use super::packet::RawPacket;
use super::util::{bprint_to_string, non_null};
use crate::error::{AV_NOPTS_VALUE, AVERROR_EIO, AVERROR_EOF, Error, Result, check, strerror};
use crate::sys;
use crate::types::rational::{Framerate, Rational};
use crate::types::stream_kind::StreamKind;
use std::any::Any;
use std::borrow::Cow;
use std::ffi::{CStr, CString};
use std::io::{self, Write};
use std::os::raw::{c_int, c_void};
use std::panic::{self, AssertUnwindSafe};
use std::ptr::{self, NonNull};

fn cstring(path: &str) -> Result<CString> {
    CString::new(path).map_err(|_| Error::InvalidPath)
}

/// Combine a URL with FFmpeg's decoded reason for a failing return code, so open/create
/// errors distinguish "no such file" from "invalid data" from "permission denied" instead of
/// collapsing to the bare path.
fn with_reason(url: &str, code: i32) -> String {
    format!("{url} ({})", strerror(code))
}

/// An owned demuxer context with stream info already probed.
pub(crate) struct InputFormatContext {
    ptr: NonNull<sys::AVFormatContext>,
}

impl InputFormatContext {
    /// Open `url`, then probe stream info.
    pub(crate) fn open(url: &str) -> Result<Self> {
        let curl = cstring(url)?;
        let mut raw: *mut sys::AVFormatContext = ptr::null_mut();
        // SAFETY: raw is a valid out-param; curl is valid for the call.
        let ret = unsafe { sys::avformat_open_input(&mut raw, curl.as_ptr(), ptr::null(), ptr::null_mut()) };
        if ret < 0 {
            return Err(Error::OpenInput(with_reason(url, ret)));
        }
        let ptr = non_null(raw, "AVFormatContext").map_err(|_| Error::OpenInput(url.to_owned()))?;
        let mut ctx = Self { ptr };
        // SAFETY: ctx.ptr is valid; probe the streams.
        check(unsafe { sys::avformat_find_stream_info(ctx.as_mut_ptr(), ptr::null_mut()) })?;
        Ok(ctx)
    }

    pub(crate) fn as_mut_ptr(&mut self) -> *mut sys::AVFormatContext {
        self.ptr.as_ptr()
    }

    fn ctx(&self) -> *mut sys::AVFormatContext {
        self.ptr.as_ptr()
    }

    /// Number of streams.
    pub(crate) fn stream_count(&self) -> usize {
        unsafe { (*self.ctx()).nb_streams as usize }
    }

    fn stream(&self, index: usize) -> Result<*mut sys::AVStream> {
        if index >= self.stream_count() {
            return Err(Error::StreamOutOfRange(index));
        }
        // SAFETY: index is in bounds; streams is an array of nb_streams pointers.
        Ok(unsafe { *(*self.ctx()).streams.add(index) })
    }

    /// The `codecpar` of stream `index`.
    pub(crate) fn stream_codecpar(&self, index: usize) -> Result<*const sys::AVCodecParameters> {
        let s = self.stream(index)?;
        Ok(unsafe { (*s).codecpar })
    }

    /// The time base of stream `index`.
    pub(crate) fn stream_time_base(&self, index: usize) -> Result<Rational> {
        let s = self.stream(index)?;
        Ok(Rational::from_av(unsafe { (*s).time_base }))
    }

    /// The average frame rate of stream `index` (may be 0/0 if unknown).
    pub(crate) fn stream_avg_frame_rate(&self, index: usize) -> Result<Rational> {
        let s = self.stream(index)?;
        Ok(Rational::from_av(unsafe { (*s).avg_frame_rate }))
    }

    /// The media kind of stream `index`.
    pub(crate) fn stream_kind(&self, index: usize) -> Result<StreamKind> {
        let par = self.stream_codecpar(index)?;
        Ok(StreamKind::from_av(unsafe { (*par).codec_type }))
    }

    /// The codec id of stream `index`.
    pub(crate) fn stream_codec_id(&self, index: usize) -> Result<sys::AVCodecID> {
        let par = self.stream_codecpar(index)?;
        Ok(unsafe { (*par).codec_id })
    }

    /// `(width, height)` of stream `index` (both `0` for non-video streams).
    pub(crate) fn stream_dimensions(&self, index: usize) -> Result<(i32, i32)> {
        let par = self.stream_codecpar(index)?;
        Ok(unsafe { ((*par).width, (*par).height) })
    }

    /// The audio sample rate of stream `index` (`0` for non-audio streams).
    pub(crate) fn stream_sample_rate(&self, index: usize) -> Result<i32> {
        let par = self.stream_codecpar(index)?;
        Ok(unsafe { (*par).sample_rate })
    }

    /// The rotation stream `index` asks to be shown with, as FFmpeg's counterclockwise angle in degrees, from the
    /// display matrix in its side data; `None` when it has none.
    pub(crate) fn stream_display_rotation(&self, index: usize) -> Result<Option<f64>> {
        let par = self.stream_codecpar(index)?;
        // SAFETY: par is the stream's live codecpar, whose coded side data array has nb_coded_side_data entries; a
        // display matrix is nine i32s, which the size check confirms before av_display_rotation_get reads them.
        let angle = unsafe {
            let sd = sys::av_packet_side_data_get(
                (*par).coded_side_data,
                (*par).nb_coded_side_data,
                sys::AVPacketSideDataType_AV_PKT_DATA_DISPLAYMATRIX,
            );
            if sd.is_null() || (*sd).size < 9 * std::mem::size_of::<i32>() {
                return Ok(None);
            }
            sys::av_display_rotation_get((*sd).data.cast())
        };
        Ok((!angle.is_nan()).then_some(angle))
    }

    /// FFmpeg's name for stream `index`'s codec (`h264`, `mpeg4`, `pcm_s16le`, …), for any codec id.
    pub(crate) fn stream_codec_name(&self, index: usize) -> Result<String> {
        let id = self.stream_codec_id(index)?;
        // SAFETY: avcodec_get_name never returns null; it falls back to a static "unknown_codec".
        Ok(unsafe { CStr::from_ptr(sys::avcodec_get_name(id)) }.to_string_lossy().into_owned())
    }

    /// The RFC 6381 codec string FFmpeg builds for stream `index` (`avc1.640028`, `mp4a.40.2`, …), or `None` when
    /// it has none for the codec or its parameters are too incomplete to build one.
    pub(crate) fn stream_codec_string(&self, index: usize, frame_rate: Option<Framerate>) -> Result<Option<String>> {
        let par = self.stream_codecpar(index)?;
        // `1/0` is av_mime_codec_str's documented "frame rate unknown".
        let frame_rate = frame_rate.map_or(Rational::new(1, 0), |f| f.0);
        // SAFETY: par is the stream's live codecpar; bp is the live AVBPrint bprint_to_string hands over.
        let text = bprint_to_string(|bp| unsafe { sys::av_mime_codec_str(par, frame_rate.to_av(), bp) });
        Ok(text.ok().filter(|s| !s.is_empty()))
    }

    /// The demuxer's name, comma-separated when it serves several containers (`mov,mp4,m4a,3gp,3g2,mj2`).
    pub(crate) fn format_name(&self) -> String {
        // SAFETY: avformat_open_input set iformat to a static AVInputFormat, whose name is never null.
        unsafe { CStr::from_ptr((*(*self.ctx()).iformat).name) }.to_string_lossy().into_owned()
    }

    /// Total duration in seconds (estimated by the demuxer), or `0.0` if unknown.
    pub(crate) fn duration_secs(&self) -> f64 {
        let d = unsafe { (*self.ctx()).duration };
        if d == AV_NOPTS_VALUE { 0.0 } else { d as f64 / sys::AV_TIME_BASE as f64 }
    }

    /// Index of the best stream of `kind`, if any.
    pub(crate) fn best_stream(&self, kind: StreamKind) -> Option<usize> {
        // SAFETY: ctx is valid; passing no decoder out-param.
        let ret = unsafe { sys::av_find_best_stream(self.ctx(), kind.to_av(), -1, -1, ptr::null_mut(), 0) };
        if ret < 0 { None } else { Some(ret as usize) }
    }

    /// Seek within `stream_index` so that reading resumes at the keyframe at or before `ts`
    /// (in that stream's time base). Decoders must be flushed afterwards, then decoded forward
    /// to reach the exact target frame. Bracketing `max_ts` at `ts` guarantees we land on or
    /// before the target rather than overshooting.
    pub(crate) fn seek(&mut self, stream_index: usize, ts: i64) -> Result<()> {
        // SAFETY: ctx is valid; the stream index is validated by the caller.
        check(unsafe { sys::avformat_seek_file(self.ctx(), stream_index as i32, i64::MIN, ts, ts, 0) })
    }

    /// Read the next packet into `pkt`. Returns `Ok(false)` at end of input.
    pub(crate) fn read_packet(&mut self, pkt: &mut RawPacket) -> Result<bool> {
        // SAFETY: ctx is valid; pkt is a valid owned packet.
        let ret = unsafe { sys::av_read_frame(self.ctx(), pkt.as_mut_ptr()) };
        if ret == AVERROR_EOF { Ok(false) } else { check(ret).map(|_| true) }
    }
}

impl Drop for InputFormatContext {
    fn drop(&mut self) {
        let mut ptr = self.ptr.as_ptr();
        // SAFETY: avformat_close_input takes a pointer-to-pointer and nulls it.
        unsafe { sys::avformat_close_input(&mut ptr) };
    }
}

// SAFETY: a single owner with no shared interior state.
unsafe impl Send for InputFormatContext {}

/// Where an [`OutputFormatContext`] writes its bytes.
pub(crate) enum OutputTarget {
    /// A file or URL, opened by FFmpeg.
    Path(String),
    /// The caller's writer, behind a custom AVIO.
    Writer(Box<dyn Write + Send>),
}

/// How an output context's AVIO is owned, which decides its teardown.
enum OutputIo {
    /// The muxer writes no file, or nothing was opened yet.
    None,
    /// FFmpeg opened a file with `avio_open`.
    File,
    /// A custom AVIO writing into a [`WriteSink`] that the context owns.
    Sink(Box<WriteSink>),
}

/// The custom AVIO's `opaque`: the caller's writer, plus the first error it returned.
struct WriteSink {
    writer: Box<dyn Write + Send>,
    /// The writer's first error, waiting to be returned as [`Error::Write`].
    error: Option<io::Error>,
    /// Set once the writer has failed, so later writes are refused without calling it again.
    failed: bool,
}

/// The custom AVIO's buffer size: FFmpeg's own default for file AVIO.
const SINK_BUFFER_SIZE: usize = 64 * 1024;

/// The custom AVIO's write callback: hand `buf` to the sink's writer. A failed or panicking write is stored on
/// the sink and reported to FFmpeg as `EIO`, so no panic unwinds across the FFI boundary.
unsafe extern "C" fn write_to_sink(opaque: *mut c_void, buf: *const u8, size: c_int) -> c_int {
    // SAFETY: opaque is the context's live `WriteSink`, and FFmpeg calls this from the thread driving the muxer,
    // which holds the context exclusively.
    let sink = unsafe { &mut *opaque.cast::<WriteSink>() };
    if sink.failed {
        return AVERROR_EIO;
    }
    let data = match usize::try_from(size) {
        // SAFETY: FFmpeg passes a buffer of `size` readable bytes.
        Ok(len) if len > 0 => unsafe { std::slice::from_raw_parts(buf, len) },
        _ => return 0,
    };
    let result = panic::catch_unwind(AssertUnwindSafe(|| sink.writer.write_all(data)))
        .unwrap_or_else(|payload| Err(io::Error::other(format!("the writer panicked: {}", panic_message(&*payload)))));
    match result {
        Ok(()) => size,
        Err(e) => {
            sink.error = Some(e);
            sink.failed = true;
            AVERROR_EIO
        }
    }
}

/// The text of a panic payload, when it carries one.
fn panic_message(payload: &(dyn Any + Send)) -> &str {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("no message")
}

/// An owned muxer context, writing to a path or to the caller's writer.
pub(crate) struct OutputFormatContext {
    ptr: NonNull<sys::AVFormatContext>,
    io: OutputIo,
}

impl OutputFormatContext {
    /// Allocate an output context writing to `target`, in the container `format` names, or, for a path, the one its
    /// extension implies.
    pub(crate) fn create(target: OutputTarget, format: Option<&str>) -> Result<Self> {
        let cformat = format.map(|f| CString::new(f).map_err(|_| Error::InvalidConfig("format contains a NUL byte")));
        let cformat = cformat.transpose()?;
        let format_ptr = cformat.as_ref().map_or(ptr::null(), |f| f.as_ptr());
        let (label, curl) = match &target {
            OutputTarget::Path(url) => (Cow::Borrowed(url.as_str()), Some(cstring(url)?)),
            OutputTarget::Writer(_) => (Cow::Owned(format!("writer ({})", format.unwrap_or("no format"))), None),
        };
        let url_ptr = curl.as_ref().map_or(ptr::null(), |u| u.as_ptr());

        let mut raw: *mut sys::AVFormatContext = ptr::null_mut();
        // SAFETY: raw is a valid out-param; the format name and filename are valid C strings or null.
        let ret = unsafe { sys::avformat_alloc_output_context2(&mut raw, ptr::null(), format_ptr, url_ptr) };
        if ret < 0 || raw.is_null() {
            return Err(Error::CreateOutput(with_reason(&label, ret)));
        }
        let mut ctx = Self { ptr: non_null(raw, "AVFormatContext")?, io: OutputIo::None };

        // A file-less muxer (e.g. a pipe/protocol format) needs no AVIO.
        let needs_file = unsafe {
            let oformat = (*ctx.ctx()).oformat;
            (*oformat).flags & sys::AVFMT_NOFILE as i32 == 0
        };
        match (target, curl) {
            (OutputTarget::Path(url), Some(curl)) if needs_file => {
                // SAFETY: pb is a valid out-param slot; curl valid for the call.
                let r = unsafe { sys::avio_open(&mut (*ctx.ctx()).pb, curl.as_ptr(), sys::AVIO_FLAG_WRITE as i32) };
                if r < 0 {
                    return Err(Error::CreateOutput(with_reason(&url, r)));
                }
                ctx.io = OutputIo::File;
            }
            (OutputTarget::Path(_), _) => {}
            (OutputTarget::Writer(_), _) if !needs_file => {
                return Err(Error::InvalidConfig("this format writes no bytes, so it can't write to a writer"));
            }
            (OutputTarget::Writer(writer), _) => ctx.attach_sink(writer)?,
        }
        Ok(ctx)
    }

    /// Point the context's `pb` at a custom, non-seekable AVIO that writes into `writer`.
    fn attach_sink(&mut self, writer: Box<dyn Write + Send>) -> Result<()> {
        let mut sink = Box::new(WriteSink { writer, error: None, failed: false });
        // SAFETY: a plain allocation, handed to the AVIO, which frees it on teardown.
        let buffer = non_null(unsafe { sys::av_malloc(SINK_BUFFER_SIZE) }, "AVIO buffer")?;
        let opaque: *mut WriteSink = &mut *sink;
        // SAFETY: buffer is SINK_BUFFER_SIZE bytes; opaque outlives the AVIO, since `Drop` frees the AVIO before the
        // sink; there is no read or seek callback.
        let pb = unsafe {
            sys::avio_alloc_context(
                buffer.as_ptr().cast(),
                SINK_BUFFER_SIZE as c_int,
                1,
                opaque.cast(),
                None,
                Some(write_to_sink),
                None,
            )
        };
        let Some(pb) = NonNull::new(pb) else {
            // SAFETY: the AVIO wasn't created, so the buffer is still ours to free.
            unsafe { sys::av_free(buffer.as_ptr()) };
            return Err(Error::AllocFailed("AVIOContext"));
        };
        // SAFETY: ctx and pb are valid. `seekable = 0` tells muxers not to seek back; CUSTOM_IO tells FFmpeg not to
        // close an AVIO it didn't open.
        unsafe {
            (*pb.as_ptr()).seekable = 0;
            (*self.ctx()).pb = pb.as_ptr();
            (*self.ctx()).flags |= sys::AVFMT_FLAG_CUSTOM_IO as i32;
        }
        self.io = OutputIo::Sink(sink);
        Ok(())
    }

    /// Turn an FFmpeg return code into a `Result`, preferring the writer's own error when it has failed.
    fn check_io(&mut self, code: i32) -> Result<()> {
        if let OutputIo::Sink(sink) = &mut self.io
            && let Some(e) = sink.error.take()
        {
            return Err(Error::Write(e));
        }
        check(code)
    }

    fn ctx(&self) -> *mut sys::AVFormatContext {
        self.ptr.as_ptr()
    }

    /// `true` if the container wants codec extradata in the header (MP4, MKV, …), meaning
    /// encoders feeding it must set the global-header flag.
    pub(crate) fn wants_global_header(&self) -> bool {
        unsafe {
            let oformat = (*self.ctx()).oformat;
            (*oformat).flags & sys::AVFMT_GLOBALHEADER as i32 != 0
        }
    }

    /// The container's default audio codec id (used when auto-encoding audio that can't be
    /// stream-copied). `AV_CODEC_ID_NONE` if the container holds no audio.
    pub(crate) fn default_audio_codec_id(&self) -> sys::AVCodecID {
        // SAFETY: oformat is a valid static AVOutputFormat.
        unsafe { (*(*self.ctx()).oformat).audio_codec }
    }

    /// `true` if this container accepts `codec_id` (e.g. can AAC go into this muxer?).
    pub(crate) fn supports_codec(&self, codec_id: sys::AVCodecID) -> bool {
        // SAFETY: oformat is valid; query_codec is a pure lookup. Returns 1 (yes), 0 (no), or
        // <0 (unknown) — treat only an explicit 1 as supported.
        let ret =
            unsafe { sys::avformat_query_codec((*self.ctx()).oformat, codec_id, sys::FF_COMPLIANCE_NORMAL as i32) };
        ret == 1
    }

    /// Add an output stream that copies `par` verbatim (for stream-copy/remux). The codec
    /// tag is cleared so the target muxer assigns a compatible one.
    pub(crate) fn add_stream_copy(&mut self, par: *const sys::AVCodecParameters) -> Result<usize> {
        let index = self.add_stream()?;
        let s = self.stream(index)?;
        // SAFETY: s is a valid stream just created; par is a valid source codecpar.
        check(unsafe { sys::avcodec_parameters_copy((*s).codecpar, par) })?;
        unsafe { (*(*s).codecpar).codec_tag = 0 };
        Ok(index)
    }

    /// Add a new output stream and return its index.
    pub(crate) fn add_stream(&mut self) -> Result<usize> {
        // SAFETY: ctx is valid; passing null codec lets us fill codecpar ourselves.
        let s = unsafe { sys::avformat_new_stream(self.ctx(), ptr::null()) };
        let s = non_null(s, "AVStream")?;
        Ok(unsafe { (*s.as_ptr()).index as usize })
    }

    fn stream(&self, index: usize) -> Result<*mut sys::AVStream> {
        let count = unsafe { (*self.ctx()).nb_streams as usize };
        if index >= count {
            return Err(Error::StreamOutOfRange(index));
        }
        Ok(unsafe { *(*self.ctx()).streams.add(index) })
    }

    /// Copy an opened encoder's parameters into output stream `index`.
    pub(crate) fn set_stream_params(&mut self, index: usize, enc: &CodecContext) -> Result<()> {
        let s = self.stream(index)?;
        enc.write_params(unsafe { (*s).codecpar })?;
        // Seed the stream time base from the encoder; the muxer may refine it at header time.
        unsafe { (*s).time_base = enc.time_base().to_av() };
        Ok(())
    }

    /// The time base of output stream `index` (read **after** `write_header`).
    pub(crate) fn stream_time_base(&self, index: usize) -> Result<Rational> {
        let s = self.stream(index)?;
        Ok(Rational::from_av(unsafe { (*s).time_base }))
    }

    /// Write the container header, passing `options` to the muxer. Options are applied before any byte is written,
    /// and any the muxer didn't recognise fail with [`Error::UnknownOption`].
    pub(crate) fn write_header(&mut self, options: &[(CString, CString)]) -> Result<()> {
        let mut dict = Dictionary::new(options)?;
        // SAFETY: ctx is valid with all streams configured; init_output consumes the options it recognises and
        // leaves the rest in the dictionary.
        let ret = unsafe { sys::avformat_init_output(self.ctx(), &mut dict.0) };
        self.check_io(ret)?;
        dict.ensure_consumed()?;
        // SAFETY: the muxer was initialised above, so no options are left to pass.
        let ret = unsafe { sys::avformat_write_header(self.ctx(), ptr::null_mut()) };
        self.check_io(ret)
    }

    /// Interleave and write a packet (whose `stream_index` must already be set).
    pub(crate) fn write_packet(&mut self, pkt: &mut RawPacket) -> Result<()> {
        // SAFETY: ctx is valid; pkt is a valid owned packet with a set stream_index.
        let ret = unsafe { sys::av_interleaved_write_frame(self.ctx(), pkt.as_mut_ptr()) };
        self.check_io(ret)
    }

    /// Finalise the file.
    pub(crate) fn write_trailer(&mut self) -> Result<()> {
        // SAFETY: ctx is valid and the header was written.
        // av_write_trailer also flushes the AVIO.
        let ret = unsafe { sys::av_write_trailer(self.ctx()) };
        self.check_io(ret)
    }

    /// Write out everything muxed so far: drain the interleaving queue, ask the muxer to close what it has pending
    /// (the mp4 muxer cuts its current fragment; a muxer that can't flush on demand ignores it), then hand the
    /// AVIO's buffer to the file or writer.
    pub(crate) fn flush(&mut self) -> Result<()> {
        // SAFETY: ctx is valid and its header was written; a null packet means "flush" to both calls.
        let ret = unsafe { sys::av_interleaved_write_frame(self.ctx(), ptr::null_mut()) };
        self.check_io(ret)?;
        let ret = unsafe { sys::av_write_frame(self.ctx(), ptr::null_mut()) };
        self.check_io(ret)?;
        self.flush_io()
    }

    /// Hand what the AVIO has buffered to its file or writer.
    fn flush_io(&mut self) -> Result<()> {
        // SAFETY: ctx is valid; pb is null for a file-less muxer, and avio_flush sets pb->error on failure.
        let ret = unsafe {
            let pb = (*self.ctx()).pb;
            if pb.is_null() {
                return Ok(());
            }
            sys::avio_flush(pb);
            (*pb).error
        };
        self.check_io(ret)
    }
}

impl Drop for OutputFormatContext {
    fn drop(&mut self) {
        // Close the AVIO first, then free the context.
        match std::mem::replace(&mut self.io, OutputIo::None) {
            OutputIo::None => {}
            OutputIo::File => {
                // SAFETY: pb was opened by avio_open; closep nulls it.
                unsafe { sys::avio_closep(&mut (*self.ctx()).pb) };
            }
            OutputIo::Sink(sink) => {
                // SAFETY: pb is the custom AVIO from attach_sink. Flush it into the writer while the sink is alive,
                // free its buffer (FFmpeg may have reallocated it), then the AVIO itself.
                unsafe {
                    let pb = &mut (*self.ctx()).pb;
                    sys::avio_flush(*pb);
                    sys::av_freep((&mut (**pb).buffer as *mut *mut u8).cast());
                    sys::avio_context_free(pb);
                }
                // Nothing refers to the sink any more; this drops the caller's writer.
                drop(sink);
            }
        }
        // SAFETY: ctx was allocated by avformat_alloc_output_context2.
        unsafe { sys::avformat_free_context(self.ctx()) };
    }
}

// SAFETY: a single owner with no shared interior state; the sink's writer is `Send`.
unsafe impl Send for OutputFormatContext {}
