# Raw FFI

**Use when:** you need something the safe API doesn't wrap yet. `media-rs` exposes the
complete, raw FFmpeg C bindings under [`media::sys`](../reference/index.md) as an escape
hatch. Everything there is `unsafe` — you're calling C directly.

!!! warning "Last resort"
    Reach for `media::sys` only when the safe API genuinely can't do what you need. The raw
    bindings have no lifetime, ownership, or error-handling guarantees. If you find yourself
    needing something here often, it's a good candidate for a feature request on the safe API.

## Calling a raw function

These two examples deliberately **don't** use the prelude — they work against `media::sys`
directly.

```rust
/// Decode FFmpeg's packed version integer (0xMMNNPP) into major.minor.micro.
fn parts(v: u32) -> (u32, u32, u32) {
    ((v >> 16) & 0xff, (v >> 8) & 0xff, v & 0xff)
}

// SAFETY: both are argument-less C accessors returning a packed version constant.
let (codec, format) = unsafe {
    (media::sys::avcodec_version(), media::sys::avformat_version()) // (1)!
};

let (maj, min, mic) = parts(codec);
println!("libavcodec {maj}.{min}.{mic}");
let _ = format;
```

1. `avcodec_version()` / `avformat_version()` are raw C functions from the bindings. They take
   no arguments and return a packed `u32`, so the `unsafe` block is trivially sound — but it's
   still `unsafe`, because the compiler can't verify FFI calls for you.

## The linked version (safe)

For the common "which FFmpeg is this?" question, there's a safe wrapper — no `unsafe`
required:

```rust
println!("Linked FFmpeg: {}", media::version_info()); // (1)!
```

1. `media::version_info()` returns the build version string of the linked libraries (e.g.
   `"8.1.2"`). It's a thin, safe wrapper over `sys::av_version_info` and doubles as a smoke
   test that the static libraries linked correctly.

## What's in `media::sys`

The bindings are pre-generated and committed, and cover the **full portable FFmpeg API** —
`AVFormatContext`, `AVCodecContext`, `av_*` functions, the `AVCodecID_*` and `AVPixelFormat_*`
constants, and so on. Because bindgen output is pure Rust, every platform's structs are
present on every platform; a function only fails to *link* if you actually call one that isn't
in your target's binaries.

## See also

- [API reference overview](../reference/index.md) — the module map, including `sys`
- The safe building blocks in [Types](../reference/types.md) that wrap these raw constants
