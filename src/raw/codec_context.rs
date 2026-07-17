//! RAII wrapper for `AVCodecContext` plus the shared send/receive primitives used by both
//! decoders and encoders.

use super::frame::RawFrame;
use super::packet::RawPacket;
use super::util::non_null;
use crate::error::{AVERROR_EAGAIN, AVERROR_EOF, Error, Result, check};
use crate::sys;
use crate::types::channel_layout::ChannelLayout;
use crate::types::rational::Rational;
use std::ffi::CString;
use std::ptr::{self, NonNull};

/// The outcome of a `receive_frame`/`receive_packet`/`buffersink_get_frame` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Receive {
    /// A frame/packet was produced.
    Got,
    /// The codec needs more input before it can produce output.
    Again,
    /// The codec is fully drained; no more output will come.
    Eof,
}

impl Receive {
    /// Classify an FFmpeg `receive`/`get_frame` return code: `0` is output, `EAGAIN`/`EOF`
    /// are the two non-error stop conditions, anything else negative is a real error.
    pub(crate) fn from_code(ret: i32) -> Result<Receive> {
        if ret == 0 {
            Ok(Receive::Got)
        } else if ret == AVERROR_EAGAIN {
            Ok(Receive::Again)
        } else if ret == AVERROR_EOF {
            Ok(Receive::Eof)
        } else {
            Err(Error::from_code(ret))
        }
    }
}

/// Drive one step of a receive/drain iterator: on `Got`, take the produced item; on
/// `Again`/`Eof`, stop; on error, surface it. Shared by the decode/encode iterators.
pub(crate) fn drain_received<T>(received: Result<Receive>, take: impl FnOnce() -> Result<T>) -> Option<Result<T>> {
    match received {
        Ok(Receive::Got) => Some(take()),
        Ok(Receive::Again) | Ok(Receive::Eof) => None,
        Err(e) => Some(Err(e)),
    }
}

/// Look up a decoder by codec id. Returns null if this build has no such decoder.
pub(crate) fn find_decoder(id: sys::AVCodecID) -> *const sys::AVCodec {
    // SAFETY: avcodec_find_decoder is a pure lookup over static tables.
    unsafe { sys::avcodec_find_decoder(id) }
}

/// Look up an encoder by name (e.g. `"libx264"`). Errors as [`Error::CodecUnavailable`] if
/// this FFmpeg build doesn't ship it.
pub(crate) fn find_encoder_by_name(name: &str) -> Result<*const sys::AVCodec> {
    let cname = CString::new(name).map_err(|_| Error::CodecUnavailable(name.to_owned()))?;
    // SAFETY: cname is a valid NUL-terminated string for the duration of the call.
    let codec = unsafe { sys::avcodec_find_encoder_by_name(cname.as_ptr()) };
    if codec.is_null() { Err(Error::CodecUnavailable(name.to_owned())) } else { Ok(codec) }
}

/// An owned `AVCodecContext`. Freed with `avcodec_free_context` on drop.
pub(crate) struct CodecContext {
    ptr: NonNull<sys::AVCodecContext>,
    codec: *const sys::AVCodec,
}

impl CodecContext {
    /// Allocate a context for `codec` (which must outlive the context — codecs are static).
    pub(crate) fn alloc(codec: *const sys::AVCodec) -> Result<Self> {
        // SAFETY: avcodec_alloc_context3 accepts a codec pointer (or null) and allocates.
        let ptr = unsafe { sys::avcodec_alloc_context3(codec) };
        Ok(Self { ptr: non_null(ptr, "AVCodecContext")?, codec })
    }

    #[inline]
    fn ctx(&self) -> *mut sys::AVCodecContext {
        self.ptr.as_ptr()
    }

    /// Open the codec, finalising configuration. Pass `None` for no private options.
    pub(crate) fn open(&mut self) -> Result<()> {
        // SAFETY: ctx and codec are valid; passing null options.
        check(unsafe { sys::avcodec_open2(self.ctx(), self.codec, ptr::null_mut()) })
    }

    // --- parameter copy ---------------------------------------------------------------

    /// Fill this context from stream `codecpar` (used when building a decoder).
    pub(crate) fn set_params(&mut self, par: *const sys::AVCodecParameters) -> Result<()> {
        // SAFETY: ctx is valid; par points to a valid AVCodecParameters.
        check(unsafe { sys::avcodec_parameters_to_context(self.ctx(), par) })
    }

    /// Copy this (opened) context's parameters into a stream's `codecpar`.
    pub(crate) fn write_params(&self, par: *mut sys::AVCodecParameters) -> Result<()> {
        // SAFETY: ctx is valid and opened; par is a writable AVCodecParameters.
        check(unsafe { sys::avcodec_parameters_from_context(par, self.ctx()) })
    }

    // --- setters (encoder configuration) ----------------------------------------------

    pub(crate) fn set_video_format(&mut self, width: i32, height: i32, pix_fmt: sys::AVPixelFormat) {
        // SAFETY: ctx is a valid, not-yet-opened context.
        unsafe {
            (*self.ctx()).width = width;
            (*self.ctx()).height = height;
            (*self.ctx()).pix_fmt = pix_fmt;
        }
    }

    pub(crate) fn set_time_base(&mut self, tb: Rational) {
        unsafe { (*self.ctx()).time_base = tb.to_av() };
    }

    pub(crate) fn set_framerate(&mut self, fr: Rational) {
        unsafe { (*self.ctx()).framerate = fr.to_av() };
    }

    pub(crate) fn set_bit_rate(&mut self, bit_rate: i64) {
        unsafe { (*self.ctx()).bit_rate = bit_rate };
    }

    pub(crate) fn set_gop_size(&mut self, gop: i32) {
        unsafe { (*self.ctx()).gop_size = gop };
    }

    /// Request multithreaded decoding. `count` is the worker count (`0` = auto-detect from the
    /// CPU); `thread_type` is a bitmask of `FF_THREAD_FRAME`/`FF_THREAD_SLICE`. Must be set
    /// before [`open`]; decoders that don't support the requested mode fall back silently.
    ///
    /// [`open`]: Self::open
    pub(crate) fn set_threading(&mut self, count: i32, thread_type: i32) {
        // SAFETY: ctx is a valid, not-yet-opened context.
        unsafe {
            (*self.ctx()).thread_count = count;
            (*self.ctx()).thread_type = thread_type;
        }
    }

    pub(crate) fn set_audio_format(
        &mut self,
        sample_rate: i32,
        sample_fmt: sys::AVSampleFormat,
        ch_layout: &ChannelLayout,
    ) {
        // SAFETY: ctx is valid; copy_into deep-copies the layout into the context's slot.
        unsafe {
            (*self.ctx()).sample_rate = sample_rate;
            (*self.ctx()).sample_fmt = sample_fmt;
            ch_layout.copy_into(&mut (*self.ctx()).ch_layout);
        }
    }

    /// Set the global-header flag (required by container formats like MP4/MKV so codec
    /// extradata lives in the container header rather than inline).
    pub(crate) fn set_global_header(&mut self) {
        unsafe {
            (*self.ctx()).flags |= sys::AV_CODEC_FLAG_GLOBAL_HEADER as i32;
        }
    }

    /// Set a private codec option (e.g. `"preset"`, `"profile"`) before [`open`].
    ///
    /// [`open`]: Self::open
    pub(crate) fn set_opt(&mut self, key: &str, val: &str) -> Result<()> {
        let ckey = CString::new(key).map_err(|_| Error::InvalidConfig("option key has NUL"))?;
        let cval = CString::new(val).map_err(|_| Error::InvalidConfig("option value has NUL"))?;
        // SAFETY: ctx is a valid AVClass-bearing object; strings are valid for the call.
        check(unsafe {
            sys::av_opt_set(self.ctx().cast(), ckey.as_ptr(), cval.as_ptr(), sys::AV_OPT_SEARCH_CHILDREN as i32)
        })
    }

    // --- getters ----------------------------------------------------------------------

    pub(crate) fn width(&self) -> i32 {
        unsafe { (*self.ctx()).width }
    }

    pub(crate) fn height(&self) -> i32 {
        unsafe { (*self.ctx()).height }
    }

    pub(crate) fn pix_fmt(&self) -> sys::AVPixelFormat {
        unsafe { (*self.ctx()).pix_fmt }
    }

    pub(crate) fn sample_rate(&self) -> i32 {
        unsafe { (*self.ctx()).sample_rate }
    }

    pub(crate) fn sample_fmt(&self) -> sys::AVSampleFormat {
        unsafe { (*self.ctx()).sample_fmt }
    }

    /// The encoder's required samples-per-frame, or `0` when the codec accepts variable-size
    /// frames (e.g. FLAC, PCM). Valid only after [`open`](Self::open).
    pub(crate) fn frame_size(&self) -> i32 {
        unsafe { (*self.ctx()).frame_size }
    }

    /// `true` if the codec accepts a variable number of samples per frame.
    pub(crate) fn accepts_variable_frame_size(&self) -> bool {
        // SAFETY: codec is a valid static AVCodec pointer for this context's lifetime.
        let caps = unsafe { (*self.codec).capabilities } as u32;
        caps & sys::AV_CODEC_CAP_VARIABLE_FRAME_SIZE != 0
    }

    /// The sample formats this (encoder) codec accepts, newest FFmpeg-8 query API. An empty
    /// vec means "unknown / all formats" — callers should fall back to a sensible default.
    pub(crate) fn supported_sample_formats(&self) -> Vec<sys::AVSampleFormat> {
        let mut out: *const std::os::raw::c_void = ptr::null();
        let mut n: i32 = 0;
        // SAFETY: ctx and codec are valid; out/n are valid out-params. On success `out` points
        // to `n` AVSampleFormat values owned by FFmpeg (we only read them).
        let ret = unsafe {
            sys::avcodec_get_supported_config(
                self.ctx(),
                self.codec,
                sys::AVCodecConfig_AV_CODEC_CONFIG_SAMPLE_FORMAT,
                0,
                &mut out,
                &mut n,
            )
        };
        if ret < 0 || out.is_null() || n <= 0 {
            return Vec::new();
        }
        let fmts = out as *const sys::AVSampleFormat;
        // SAFETY: `fmts` points to `n` valid AVSampleFormat entries.
        (0..n as usize).map(|i| unsafe { *fmts.add(i) }).collect()
    }

    pub(crate) fn time_base(&self) -> Rational {
        Rational::from_av(unsafe { (*self.ctx()).time_base })
    }

    pub(crate) fn framerate(&self) -> Rational {
        Rational::from_av(unsafe { (*self.ctx()).framerate })
    }

    /// Deep-copy this context's channel layout (for inheriting from a decoder).
    pub(crate) fn ch_layout_owned(&self) -> ChannelLayout {
        ChannelLayout::copy_from(unsafe { &(*self.ctx()).ch_layout })
    }

    // --- decode side ------------------------------------------------------------------

    /// Submit a packet (`None` flushes the decoder).
    pub(crate) fn send_packet(&mut self, pkt: Option<&RawPacket>) -> Result<()> {
        let p = pkt.map_or(ptr::null(), |p| p.as_ptr());
        // SAFETY: ctx is valid+open; p is null or a valid packet.
        let ret = unsafe { sys::avcodec_send_packet(self.ctx(), p) };
        if ret == AVERROR_EOF {
            Ok(()) // already flushed; harmless
        } else {
            check(ret)
        }
    }

    /// Drop the codec's buffered state. Called after seeking so stale frames from before the
    /// seek don't leak into the output.
    pub(crate) fn flush_buffers(&mut self) {
        // SAFETY: ctx is valid and open.
        unsafe { sys::avcodec_flush_buffers(self.ctx()) };
    }

    /// Receive one decoded frame into `frame`.
    pub(crate) fn receive_frame(&mut self, frame: &mut RawFrame) -> Result<Receive> {
        // SAFETY: ctx is valid+open; frame is a valid owned frame.
        let ret = unsafe { sys::avcodec_receive_frame(self.ctx(), frame.as_mut_ptr()) };
        Receive::from_code(ret)
    }

    // --- encode side ------------------------------------------------------------------

    /// Submit a frame to the encoder (`None` flushes it).
    pub(crate) fn send_frame(&mut self, frame: Option<&RawFrame>) -> Result<()> {
        let f = frame.map_or(ptr::null(), |f| f.as_ptr());
        // SAFETY: ctx is valid+open; f is null or a valid frame.
        let ret = unsafe { sys::avcodec_send_frame(self.ctx(), f) };
        if ret == AVERROR_EOF { Ok(()) } else { check(ret) }
    }

    /// Receive one encoded packet into `pkt`.
    pub(crate) fn receive_packet(&mut self, pkt: &mut RawPacket) -> Result<Receive> {
        // SAFETY: ctx is valid+open; pkt is a valid owned packet.
        let ret = unsafe { sys::avcodec_receive_packet(self.ctx(), pkt.as_mut_ptr()) };
        Receive::from_code(ret)
    }
}

impl Drop for CodecContext {
    fn drop(&mut self) {
        let mut ptr = self.ptr.as_ptr();
        // SAFETY: avcodec_free_context takes a pointer-to-pointer and nulls it.
        unsafe { sys::avcodec_free_context(&mut ptr) };
    }
}

// SAFETY: a CodecContext uniquely owns its AVCodecContext; FFmpeg codec contexts are not
// internally synchronised, so Send (move across threads) is sound but Sync is not.
unsafe impl Send for CodecContext {}
