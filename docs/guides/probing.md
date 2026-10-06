# Probing media

**Use when:** you need a file's duration, stream layout, resolution or codecs — and you don't
want to pay to decode it. `probe` opens the container, reads its metadata, and returns.

## The whole thing

```rust
use media::prelude::*;

let info = probe("input.mp4")?; // (1)!

println!("Duration: {:.2}s", info.duration().as_secs_f64()); // (2)!
println!("Container: {}", info.format_name());
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
   the fields that apply depend on the kind: `width`/`height`/`video_codec`/`frame_rate` for video,
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
| `frame_rate` | `Option<Framerate>` | The container's average frame rate (video; `None` otherwise, or when the container declares none). `as_f64()` gives frames per second. |
| `codec_name` | `String` | FFmpeg's name for the codec, for **any** codec: `h264`, `aac`, `mpeg4`, `wmv3`, `pcm_s16le`, … |
| `codec_string` | `Option<String>` | The RFC 6381 codec string, such as `avc1.640028` or `mp4a.40.2`. See below. |

!!! note "Unknown codecs are `None`"
    `video_codec`/`audio_codec` are `Option`s because `media-rs` only enumerates a known set
    of codecs (see [Types](../reference/types.md)). A stream in a codec the crate doesn't
    model yet still appears, with its `kind` and dimensions, but a `None` codec.

## Container name and codec strings

These three answer "can a browser play this?" without decoding anything:

```rust
use media::prelude::*;

let info = probe("input.mp4")?;
println!("{}", info.format_name());                  // (1)!

for stream in info.streams() {
    println!("{} → {:?}", stream.codec_name, stream.codec_string); // (2)!
}
# Ok::<(), media::Error>(())
```

1. `format_name()` is the name of FFmpeg's demuxer, for example `mov,mp4,m4a,3gp,3g2,mj2` or `matroska,webm`.
   One demuxer reads several related containers, so it **can't tell MP4 from MOV, or WebM from Matroska**. Don't
   pick a MIME type from it alone.
2. `codec_name` names every codec, even those `video_codec`/`audio_codec` leave as `None`. `codec_string` is
   what goes in a MIME type's `codecs=` parameter, for example `video/mp4; codecs="avc1.640028, mp4a.40.2"`,
   which is what a web view's `canPlayType` or `MediaSource.isTypeSupported` wants.

The codec strings are FFmpeg's own (`av_mime_codec_str`), so they follow FFmpeg's coverage:

- **H.264, HEVC, AV1, AAC, MP3, AC-3, E-AC-3** get full RFC 6381 strings: `avc1.64001f`, `hvc1.1.6.L120.90`,
  `av01.0.08M.08`, `mp4a.40.2`, `mp4a.40.34`, `ac-3`, `ec-3`.
- **VP8, VP9, Vorbis, Opus, FLAC** get WebM's short names: `vp8`, `vp09.00.40.08` (or just `vp9` when its profile
  can't be read), `vorbis`, `opus`, `flac`.
- **MPEG-4 Part 2** gets `mp4v.20`, which leaves out the profile and level.
- **`None`** means FFmpeg has no string for the codec (WMV, ProRes, PCM, …), or the stream's parameters are too
  incomplete to build one: H.264 without `avcC` extradata, or HEVC tagged `hev1` rather than `hvc1`. Treat `None`
  as "can't tell, assume it won't play".

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
