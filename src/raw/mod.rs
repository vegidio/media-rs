//! Private RAII layer: thin owners over FFmpeg's heap-allocated objects.
//!
//! This is the **only** module that calls the raw FFI allocators/finalizers directly. Each
//! wrapper guarantees its pointer is non-null for its lifetime and freed exactly once on
//! drop. Everything above this layer manipulates media through these types and never sees a
//! raw pointer.

pub(crate) mod codec_context;
pub(crate) mod filter_graph;
pub(crate) mod format_context;
pub(crate) mod frame;
pub(crate) mod packet;
mod util;
