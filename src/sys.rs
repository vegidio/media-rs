//! Raw, auto-generated FFI bindings to the FFmpeg libraries.
//!
//! The contents of this module are produced by `bindgen` at build time from
//! `wrapper.h` and written to `$OUT_DIR/bindings.rs`. They cover the *portable core*
//! of the FFmpeg public API (avcodec, avformat, avutil, avfilter, avdevice, swscale,
//! swresample, postproc) — the surface that is identical on every platform.
//!
//! Vendor-specific hardware-context bindings (`hwcontext_cuda`, `hwcontext_d3d11va`,
//! `hwcontext_vaapi`, …) are intentionally absent: each requires an OS/SDK header that
//! is not available on every build machine. They will be generated per-OS in CI and
//! exposed here later, e.g. `pub mod hwcontext;`.

#![allow(non_upper_case_globals, non_camel_case_types, non_snake_case, dead_code)]
// `bindgen` emits transmute-based bitfield accessors that newer rustc flags as redundant.
#![allow(unnecessary_transmutes)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
