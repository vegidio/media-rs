//! RAII wrapper for a video `AVFilterGraph` wired as `buffer → … → buffersink`.
//!
//! The buffer source and sink `AVFilterContext`s are owned by the graph (freed when the
//! graph is freed), so we hold them as borrowed pointers, not separate owners.

use super::frame::RawFrame;
use super::util::non_null;
use crate::error::{Error, Result, check};
use crate::raw::codec_context::Receive;
use crate::sys;
use crate::types::channel_layout::ChannelLayout;
use crate::types::rational::Rational;
use std::ffi::CString;
use std::ptr::{self, NonNull};

/// Parameters describing the frames that will be pushed into the graph.
pub(crate) struct VideoInput {
    pub width: i32,
    pub height: i32,
    pub pix_fmt: sys::AVPixelFormat,
    pub time_base: Rational,
    pub sample_aspect_ratio: Rational,
}

/// A configured video filter graph.
pub(crate) struct VideoFilterGraph {
    graph: NonNull<sys::AVFilterGraph>,
    src: *mut sys::AVFilterContext,
    sink: *mut sys::AVFilterContext,
}

fn get_filter(name: &str) -> Result<*const sys::AVFilter> {
    let cname = CString::new(name).map_err(|_| Error::InvalidConfig("filter name has NUL"))?;
    // SAFETY: pure lookup over static filter tables.
    let f = unsafe { sys::avfilter_get_by_name(cname.as_ptr()) };
    if f.is_null() {
        Err(Error::InvalidConfig("required filter (buffer/buffersink) is unavailable"))
    } else {
        Ok(f)
    }
}

/// Parse `filters` between the already-created buffer source (`src`, labelled `"in"`) and sink
/// (`sink`, labelled `"out"`) and finalise the graph. Shared by the video and audio graphs —
/// the endpoints differ only in which buffer/buffersink filter created `src`/`sink`.
fn link_and_config(
    graph: *mut sys::AVFilterGraph,
    src: *mut sys::AVFilterContext,
    sink: *mut sys::AVFilterContext,
    filters: &str,
) -> Result<()> {
    let in_name = CString::new("in").unwrap();
    let out_name = CString::new("out").unwrap();
    let cfilters = CString::new(filters).map_err(|_| Error::InvalidConfig("filter description has NUL"))?;
    // SAFETY: inout_alloc returns a node or null; names are av_strdup'd so inout_free can
    // release them.
    unsafe {
        let outputs = non_null(sys::avfilter_inout_alloc(), "AVFilterInOut")?;
        let inputs = match non_null(sys::avfilter_inout_alloc(), "AVFilterInOut") {
            Ok(i) => i,
            // The first node allocated but the second didn't; free the first (it has no Drop
            // guard) before propagating so we don't leak it.
            Err(e) => {
                let mut outputs_p = outputs.as_ptr();
                sys::avfilter_inout_free(&mut outputs_p);
                return Err(e);
            }
        };

        (*outputs.as_ptr()).name = sys::av_strdup(in_name.as_ptr());
        (*outputs.as_ptr()).filter_ctx = src;
        (*outputs.as_ptr()).pad_idx = 0;
        (*outputs.as_ptr()).next = ptr::null_mut();

        (*inputs.as_ptr()).name = sys::av_strdup(out_name.as_ptr());
        (*inputs.as_ptr()).filter_ctx = sink;
        (*inputs.as_ptr()).pad_idx = 0;
        (*inputs.as_ptr()).next = ptr::null_mut();

        let mut inputs_p = inputs.as_ptr();
        let mut outputs_p = outputs.as_ptr();
        let parse = sys::avfilter_graph_parse_ptr(graph, cfilters.as_ptr(), &mut inputs_p, &mut outputs_p, ptr::null_mut());
        // parse_ptr leaves the lists for us to free regardless of success.
        sys::avfilter_inout_free(&mut inputs_p);
        sys::avfilter_inout_free(&mut outputs_p);
        check(parse)?;

        check(sys::avfilter_graph_config(graph, ptr::null_mut()))?;
    }
    Ok(())
}

impl VideoFilterGraph {
    /// Build a graph that pushes `input`-shaped frames through `filters` (a standard
    /// libavfilter description such as `"scale=1280:720,fps=30"`).
    pub(crate) fn new(input: &VideoInput, filters: &str) -> Result<Self> {
        // SAFETY: alloc returns a graph or null.
        let graph_ptr = unsafe { sys::avfilter_graph_alloc() };
        let graph = non_null(graph_ptr, "AVFilterGraph")?;

        let sar = if input.sample_aspect_ratio.num == 0 { Rational::new(1, 1) } else { input.sample_aspect_ratio };
        let args = format!(
            "video_size={}x{}:pix_fmt={}:time_base={}/{}:pixel_aspect={}/{}",
            input.width,
            input.height,
            input.pix_fmt,
            input.time_base.num,
            input.time_base.den.max(1),
            sar.num,
            sar.den.max(1),
        );

        let mut this = Self { graph, src: ptr::null_mut(), sink: ptr::null_mut() };

        let buffer = get_filter("buffer")?;
        let buffersink = get_filter("buffersink")?;
        let in_name = CString::new("in").unwrap();
        let out_name = CString::new("out").unwrap();
        let cargs = CString::new(args).map_err(|_| Error::InvalidConfig("filter args has NUL"))?;

        // SAFETY: all pointers valid; create_filter writes the context into src/sink.
        unsafe {
            check(sys::avfilter_graph_create_filter(
                &mut this.src,
                buffer,
                in_name.as_ptr(),
                cargs.as_ptr(),
                ptr::null_mut(),
                this.graph.as_ptr(),
            ))?;
            check(sys::avfilter_graph_create_filter(
                &mut this.sink,
                buffersink,
                out_name.as_ptr(),
                ptr::null(),
                ptr::null_mut(),
                this.graph.as_ptr(),
            ))?;
        }

        link_and_config(this.graph.as_ptr(), this.src, this.sink, filters)?;
        Ok(this)
    }

    /// The width of frames the graph emits.
    pub(crate) fn out_width(&self) -> i32 {
        unsafe { sys::av_buffersink_get_w(self.sink) }
    }

    /// The height of frames the graph emits.
    pub(crate) fn out_height(&self) -> i32 {
        unsafe { sys::av_buffersink_get_h(self.sink) }
    }

    /// The pixel format of frames the graph emits.
    pub(crate) fn out_pix_fmt(&self) -> sys::AVPixelFormat {
        unsafe { sys::av_buffersink_get_format(self.sink) }
    }

    /// Push a frame into the graph (`None` signals end of stream).
    pub(crate) fn push(&mut self, frame: Option<&mut RawFrame>) -> Result<()> {
        let f = frame.map_or(ptr::null_mut(), |f| f.as_mut_ptr());
        // SAFETY: src is a valid buffer source; f is null or a valid frame.
        check(unsafe { sys::av_buffersrc_add_frame(self.src, f) })
    }

    /// Pull one filtered frame out of the graph into `frame`.
    pub(crate) fn pull(&mut self, frame: &mut RawFrame) -> Result<Receive> {
        // SAFETY: sink is a valid buffersink; frame is a valid owned frame.
        let ret = unsafe { sys::av_buffersink_get_frame(self.sink, frame.as_mut_ptr()) };
        Receive::from_code(ret)
    }
}

impl Drop for VideoFilterGraph {
    fn drop(&mut self) {
        let mut ptr = self.graph.as_ptr();
        // SAFETY: frees the graph and every filter context it owns (incl. src/sink).
        unsafe { sys::avfilter_graph_free(&mut ptr) };
    }
}

// SAFETY: single owner of the graph; not internally synchronised, so Send-only.
unsafe impl Send for VideoFilterGraph {}

/// Parameters describing the audio frames pushed into an [`AudioFilterGraph`].
pub(crate) struct AudioInput {
    pub sample_rate: i32,
    pub sample_fmt: sys::AVSampleFormat,
    pub ch_layout: ChannelLayout,
    pub time_base: Rational,
}

/// A configured audio filter graph, wired `abuffer → … → abuffersink`.
pub(crate) struct AudioFilterGraph {
    graph: NonNull<sys::AVFilterGraph>,
    src: *mut sys::AVFilterContext,
    sink: *mut sys::AVFilterContext,
}

impl AudioFilterGraph {
    /// Build a graph that pushes `input`-shaped audio frames through `filters` (a standard
    /// libavfilter description such as `"volume=0.5,highpass=f=80"`).
    pub(crate) fn new(input: &AudioInput, filters: &str) -> Result<Self> {
        // SAFETY: alloc returns a graph or null.
        let graph_ptr = unsafe { sys::avfilter_graph_alloc() };
        let graph = non_null(graph_ptr, "AVFilterGraph")?;

        let args = format!(
            "sample_rate={}:sample_fmt={}:channel_layout={}:time_base={}/{}",
            input.sample_rate,
            input.sample_fmt,
            input.ch_layout.describe(),
            input.time_base.num,
            input.time_base.den.max(1),
        );

        let mut this = Self { graph, src: ptr::null_mut(), sink: ptr::null_mut() };

        let abuffer = get_filter("abuffer")?;
        let abuffersink = get_filter("abuffersink")?;
        let in_name = CString::new("in").unwrap();
        let out_name = CString::new("out").unwrap();
        let cargs = CString::new(args).map_err(|_| Error::InvalidConfig("filter args has NUL"))?;

        // SAFETY: all pointers valid; create_filter writes the context into src/sink.
        unsafe {
            check(sys::avfilter_graph_create_filter(
                &mut this.src,
                abuffer,
                in_name.as_ptr(),
                cargs.as_ptr(),
                ptr::null_mut(),
                this.graph.as_ptr(),
            ))?;
            check(sys::avfilter_graph_create_filter(
                &mut this.sink,
                abuffersink,
                out_name.as_ptr(),
                ptr::null(),
                ptr::null_mut(),
                this.graph.as_ptr(),
            ))?;
        }

        link_and_config(this.graph.as_ptr(), this.src, this.sink, filters)?;
        Ok(this)
    }

    /// Push a frame into the graph (`None` signals end of stream).
    pub(crate) fn push(&mut self, frame: Option<&mut RawFrame>) -> Result<()> {
        let f = frame.map_or(ptr::null_mut(), |f| f.as_mut_ptr());
        // SAFETY: src is a valid abuffer source; f is null or a valid frame.
        check(unsafe { sys::av_buffersrc_add_frame(self.src, f) })
    }

    /// Pull one filtered frame out of the graph into `frame`.
    pub(crate) fn pull(&mut self, frame: &mut RawFrame) -> Result<Receive> {
        // SAFETY: sink is a valid abuffersink; frame is a valid owned frame.
        let ret = unsafe { sys::av_buffersink_get_frame(self.sink, frame.as_mut_ptr()) };
        Receive::from_code(ret)
    }
}

impl Drop for AudioFilterGraph {
    fn drop(&mut self) {
        let mut ptr = self.graph.as_ptr();
        // SAFETY: frees the graph and every filter context it owns (incl. src/sink).
        unsafe { sys::avfilter_graph_free(&mut ptr) };
    }
}

// SAFETY: single owner of the graph; not internally synchronised, so Send-only.
unsafe impl Send for AudioFilterGraph {}
