//! Raw, auto-generated FFI bindings to the FFmpeg libraries.
//!
//! The contents of this module live in the **committed** `sys/bindings.rs`, produced by
//! the `xtask` crate across a per-OS CI matrix and unioned with `xtask merge`. They cover
//! the FFmpeg core (avcodec, avformat, avutil, avfilter, avdevice, swscale, swresample,
//! and postproc when shipped) plus every platform's hardware-context surface
//! (VideoToolbox, D3D11VA/DXVA2, VAAPI/VDPAU, CUDA, QSV, OpenCL, AMF, Vulkan, …).
//!
//! Because bindgen output is pure Rust, the foreign-platform types are visible and
//! compile on every host (e.g. you can reference `AVD3D11VADeviceContext` on macOS); the
//! corresponding functions only fail to *link* if actually called on the wrong OS — which
//! the crate's higher-level API guards against.

#![allow(non_upper_case_globals, non_camel_case_types, non_snake_case, dead_code)]
// `bindgen` emits transmute-based bitfield accessors that newer rustc flags as redundant.
#![allow(unnecessary_transmutes)]
// Clippy lints that `bindgen` output trips: identity transmutes and `usize as isize` offsets
// in bitfield accessors, undocumented `unsafe` raw pointer helpers, and deeply nested function
// pointer types (e.g. `AVCodecContext::execute2`). These are inherent to generated FFI, so they
// are allowed here at the include site — the generated file cannot carry its own inner attributes
// through `include!`. `xtask merge` records the same list in the file header for future regens.
#![allow(
    clippy::missing_safety_doc,
    clippy::useless_transmute,
    clippy::ptr_offset_with_cast,
    clippy::type_complexity
)]

include!("sys/bindings.rs");
