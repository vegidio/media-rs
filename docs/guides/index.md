# Guides

Each guide is a hands-on walkthrough of one feature, with the key lines explained. They build
on the ideas in [Core concepts](../getting-started/concepts.md). Every snippet begins with
`use media::prelude::*;`.

| Guide | What you'll do |
|-------|----------------|
| [Probing media](probing.md) | Read duration, streams and codecs without decoding. |
| [Transcoding](transcoding.md) | Re-encode video across all three tiers; codecs, trim, drop streams, progress. |
| [Filters](filters.md) | Scale, denoise and color-correct with a typed filter chain. |
| [Frame extraction](frame-extraction.md) | Pull still images by time, count, fps or timestamps. |
| [Remuxing](remuxing.md) | Change container with no re-encode (stream copy). |
| [Seeking](seeking.md) | Jump to a timestamp before decoding. |
| [Low-level pipeline](low-level.md) | Wire reader → decoder → encoder → writer by hand. |
| [Logging](logging.md) | Control FFmpeg's log verbosity. |
| [Raw FFI](raw-ffi.md) | Drop down to the raw `media::sys` bindings. |

!!! tip "Runnable versions"
    The repository's [`examples/`](https://github.com/vegidio/media-rs/tree/main/examples)
    directory has a compact, runnable program for each of these. The guides here expand on
    them with line-by-line explanations.
