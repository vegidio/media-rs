//! RAII wrappers for input (demuxer) and output (muxer) `AVFormatContext`s.
//!
//! The two have **different** teardown paths: an input context is released with
//! `avformat_close_input`, while an output context needs its AVIO closed (when the format
//! writes to a file) and then `avformat_free_context`.

use super::codec_context::CodecContext;
use super::packet::RawPacket;
use super::util::non_null;
use crate::error::{AV_NOPTS_VALUE, AVERROR_EOF, Error, Result, check};
use crate::sys;
use crate::types::rational::Rational;
use crate::types::stream_kind::StreamKind;
use std::ffi::CString;
use std::ptr::{self, NonNull};

fn cstring(path: &str) -> Result<CString> {
    CString::new(path).map_err(|_| Error::InvalidPath)
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
            return Err(Error::OpenInput(url.to_owned()));
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

    /// Total duration in seconds (estimated by the demuxer), or `0.0` if unknown.
    pub(crate) fn duration_secs(&self) -> f64 {
        let d = unsafe { (*self.ctx()).duration };
        if d == AV_NOPTS_VALUE {
            0.0
        } else {
            d as f64 / sys::AV_TIME_BASE as f64
        }
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
        if ret == AVERROR_EOF {
            Ok(false)
        } else {
            check(ret).map(|_| true)
        }
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

/// An owned muxer context. Tracks whether it opened an AVIO file so drop can mirror it.
pub(crate) struct OutputFormatContext {
    ptr: NonNull<sys::AVFormatContext>,
    avio_opened: bool,
}

impl OutputFormatContext {
    /// Allocate an output context for `url`, inferring the container from its extension, and
    /// open the output file when the format requires one.
    pub(crate) fn create(url: &str) -> Result<Self> {
        let curl = cstring(url)?;
        let mut raw: *mut sys::AVFormatContext = ptr::null_mut();
        // SAFETY: raw is a valid out-param; format inferred from the filename.
        let ret = unsafe { sys::avformat_alloc_output_context2(&mut raw, ptr::null(), ptr::null(), curl.as_ptr()) };
        if ret < 0 || raw.is_null() {
            return Err(Error::CreateOutput(url.to_owned()));
        }
        let mut ctx = Self {
            ptr: non_null(raw, "AVFormatContext")?,
            avio_opened: false,
        };

        // Open the file unless the muxer is file-less (e.g. a pipe/protocol format).
        let needs_file = unsafe {
            let oformat = (*ctx.ctx()).oformat;
            (*oformat).flags & sys::AVFMT_NOFILE as i32 == 0
        };
        if needs_file {
            // SAFETY: pb is a valid out-param slot; curl valid for the call.
            let r = unsafe { sys::avio_open(&mut (*ctx.ctx()).pb, curl.as_ptr(), sys::AVIO_FLAG_WRITE as i32) };
            if r < 0 {
                return Err(Error::CreateOutput(url.to_owned()));
            }
            ctx.avio_opened = true;
        }
        Ok(ctx)
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

    /// Write the container header.
    pub(crate) fn write_header(&mut self) -> Result<()> {
        // SAFETY: ctx is valid; all streams configured.
        check(unsafe { sys::avformat_write_header(self.ctx(), ptr::null_mut()) })
    }

    /// Interleave and write a packet (whose `stream_index` must already be set).
    pub(crate) fn write_packet(&mut self, pkt: &mut RawPacket) -> Result<()> {
        // SAFETY: ctx is valid; pkt is a valid owned packet with a set stream_index.
        check(unsafe { sys::av_interleaved_write_frame(self.ctx(), pkt.as_mut_ptr()) })
    }

    /// Finalise the file.
    pub(crate) fn write_trailer(&mut self) -> Result<()> {
        // SAFETY: ctx is valid and the header was written.
        check(unsafe { sys::av_write_trailer(self.ctx()) })
    }
}

impl Drop for OutputFormatContext {
    fn drop(&mut self) {
        // Close the AVIO file first (only if we opened one), then free the context.
        if self.avio_opened {
            // SAFETY: pb was opened by avio_open; closep nulls it.
            unsafe { sys::avio_closep(&mut (*self.ctx()).pb) };
        }
        // SAFETY: ctx was allocated by avformat_alloc_output_context2.
        unsafe { sys::avformat_free_context(self.ctx()) };
    }
}

// SAFETY: a single owner with no shared interior state.
unsafe impl Send for OutputFormatContext {}
