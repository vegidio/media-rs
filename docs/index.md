# media-rs

**Encode, decode and process audio/video with FFmpeg — from safe, idiomatic Rust.**

`media-rs` is a Rust wrapper around [FFmpeg](https://ffmpeg.org) (`libavcodec`,
`libavformat`, `libavutil`, `libavfilter`, `libswscale`, `libswresample`, …). It links
**prebuilt static** FFmpeg libraries and ships **pre-generated, committed** FFI bindings, so
there is no build-time `bindgen`/`libclang`, no system FFmpeg, and no `pkg-config`.

> The crate is published as `media-rs` and imported in code as `media`.

```rust
use media::prelude::*;

// Transcode a file — container and codecs inferred from the extensions.
transcode("input.mp4").to("output.webm").run()?;
# Ok::<(), media::Error>(())
```

## Why this crate

Most FFmpeg wrappers mirror the C API one-to-one. `media-rs` does the opposite: it exposes a
**Rust-first** API that reads naturally, with a one-liner for the common case and full
frame-level control when you need it. The same task can be written three ways, and you pick
the tier that fits — see [Core concepts](getting-started/concepts.md).

<div class="grid cards" markdown>

-   :material-magnify: **Probe**

    Read a file's duration, streams, codecs and resolution without decoding a single frame.

    [:octicons-arrow-right-24: Probing media](guides/probing.md)

-   :material-movie-open-play: **Transcode**

    Re-encode video to H.264/H.265/VP9/AV1 with control over resolution, bit rate, frame
    rate, preset, profile, trimming and progress.

    [:octicons-arrow-right-24: Transcoding](guides/transcoding.md)

-   :material-image-multiple: **Extract frames**

    Pull still images from a video by time, frame count, fps or exact timestamps — to disk,
    memory, or a callback.

    [:octicons-arrow-right-24: Frame extraction](guides/frame-extraction.md)

-   :material-tune-variant: **Filter**

    Scale, force fps, denoise and color-correct with a typed, composable filter chain (or a
    raw libavfilter string).

    [:octicons-arrow-right-24: Filters](guides/filters.md)

-   :material-swap-horizontal: **Remux**

    Change a file's container with no re-encoding — a fast, lossless stream copy.

    [:octicons-arrow-right-24: Remuxing](guides/remuxing.md)

-   :material-cog: **Go low-level**

    Wire `MediaReader` → `Decoder` → `VideoEncoder` → `MediaWriter` by hand when the
    high-level API isn't enough.

    [:octicons-arrow-right-24: Low-level pipeline](guides/low-level.md)

</div>

## Feature status

A safe, idiomatic API built on the raw [`sys`](guides/raw-ffi.md) bindings is available:
probing, decoding, **video** encoding, filtering, transcoding, remuxing and frame extraction.

!!! note "Not yet available"
    Audio **re-encoding** (audio streams are copied through untouched), an `async` feature,
    and per-platform hardware guardrails are not implemented yet. The audio-codec types exist
    as typed foundations for that upcoming work.

## Next steps

1. [Install](getting-started/installation.md) the crate.
2. Run the [Quick start](getting-started/quickstart.md).
3. Learn the [three-tier API and conventions](getting-started/concepts.md).
4. Dive into the [Guides](guides/index.md) or the [API reference](reference/index.md).
