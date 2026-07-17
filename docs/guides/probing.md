# Probing media

**Use when:** you need a file's duration, stream layout, resolution or codecs — and you don't
want to pay to decode it. `probe` opens the container, reads its metadata, and returns.

## The whole thing

```rust
use media::prelude::*;

let info = probe("input.mp4")?; // (1)!

println!("Duration: {:.2}s", info.duration().as_secs_f64()); // (2)!
println!("Streams: {}", info.stream_count());

for stream in info.streams() { // (3)!
    match stream.kind { // (4)!
        StreamKind::Video => println!(
            "  [{}] video: {}x{}, codec {:?}",
            stream.index, stream.width, stream.height, stream.video_codec,
        ),
        StreamKind::Audio => println!(
            "  [{}] audio: {} Hz, codec {:?}",
            stream.index, stream.sample_rate, stream.audio_codec,
        ),
        other => println!("  [{}] {:?}", stream.index, other),
    }
}

if let Some(video) = info.video() { // (5)!
    println!("First video stream: {}x{}", video.width, video.height);
}
# Ok::<(), media::Error>(())
```

1. `probe` takes anything `AsRef<str>` (a path) and returns a
   [`MediaInfo`](../reference/probe.md). It opens the file just long enough to read the
   container header — **no frames are decoded**.
2. `duration()` is a `std::time::Duration`; `as_secs_f64()` gives seconds as a float. It is
   best-effort and may be `0` for containers that don't record a duration.
3. `streams()` returns a `&[StreamInfo]` — one entry per stream in the container, in order.
4. `stream.kind` is a [`StreamKind`](../reference/types.md#streamkind). Match on it because
   the fields that apply depend on the kind: `width`/`height`/`video_codec` for video,
   `sample_rate`/`audio_codec` for audio.
5. `video()` and `audio()` are convenience accessors returning the **first** stream of that
   kind as an `Option<&StreamInfo>`.

## What `StreamInfo` gives you

Every field is a plain public field — no getters:

| Field | Type | Meaning |
|-------|------|---------|
| `index` | `usize` | Position in the container. |
| `kind` | `StreamKind` | `Video` / `Audio` / `Subtitle` / `Data` / `Other`. |
| `width`, `height` | `u32` | Pixels (video; `0` otherwise). |
| `sample_rate` | `u32` | Hz (audio; `0` otherwise). |
| `video_codec` | `Option<VideoCodec>` | `Some` if it's a recognised video codec. |
| `audio_codec` | `Option<AudioCodec>` | `Some` if it's a recognised audio codec. |

!!! note "Unknown codecs are `None`"
    `video_codec`/`audio_codec` are `Option`s because `media-rs` only enumerates a known set
    of codecs (see [Types](../reference/types.md)). A stream in a codec the crate doesn't
    model yet still appears, with its `kind` and dimensions, but a `None` codec.

## Verifying your own output

`probe` is handy as a post-condition check after writing a file — reopen the result and
confirm it's a real, decodable container:

```rust
use media::prelude::*;
# fn demo() -> media::Result<()> {
transcode("input.mp4").to("output.mkv").run()?;

let info = probe("output.mkv")?;
assert!(info.video().is_some(), "no video stream was written");
# Ok(()) }
```

## See also

- [Probe API reference](../reference/probe.md)
- [Remuxing](remuxing.md) uses `probe` to verify the copied container.
