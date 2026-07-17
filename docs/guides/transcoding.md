# Transcoding

**Use when:** you want to re-encode video — change codec, resolution, bit rate, frame rate,
trim to a range, drop a stream, or apply filters. Transcoding follows the
[three-tier API](../getting-started/concepts.md#the-three-tier-api): a one-liner, a builder,
and (in a [separate guide](low-level.md)) the hand-wired pipeline.

!!! note "Audio is copied by default"
    By default the transcode pipeline **stream-copies** audio through untouched (fast, lossless).
    It re-encodes audio only when you ask it to — via `.audio(...)`, an audio filter, or one of
    the `.audio_*` shortcuts — or when the source codec can't fit the target container. See the
    [Audio guide](audio.md) for the full audio API, and [drop](#dropping-a-stream) either stream.

## The simplest transcode

The output container and codecs are inferred from the destination extension:

```rust
use media::prelude::*;

let summary = transcode("input.mp4").to("output.mp4").run()?; // (1)!
println!("{} frames, {:.2}s", summary.frames, summary.duration_secs); // (2)!
# Ok::<(), media::Error>(())
```

1. `transcode(input)` → `.to(output)` → `.run()`. With no other options, video is re-encoded
   to H.264 at the input's geometry and the audio is copied.
2. [`TranscodeSummary`](../reference/transcode.md#transcodesummary) reports the frames encoded
   and the output duration in seconds. Both fields are public.

## Full control with the builder

When you need to set the codec, size, bit rate, etc., build a
[`VideoConfig`](../reference/transcode.md#videoconfig) and hand it to the
[`Transcoder`](../reference/transcode.md#transcoder) builder:

```rust
use media::prelude::*;

let video = VideoConfig::builder()
    .codec(VideoCodec::H264)          // (1)!
    .resolution(640, 360)             // (2)!
    .bitrate(Bitrate::mbps(2))        // (3)!
    .framerate(Framerate::fps(30))    // (4)!
    .preset(H264Preset::Fast)         // (5)!
    .profile(H264Profile::High)       // (6)!
    .build()?;                        // (7)!

let summary = Transcoder::builder()
    .input("input.mp4")
    .output("output.mp4")
    .video(video)                     // (8)!
    .drop_audio()                     // (9)!
    .build()?
    .run()?;
# Ok::<(), media::Error>(())
```

1. `codec` is the only **required** field on a `VideoConfig`. Everything else is inherited
   from the input if omitted.
2. `resolution(w, h)` — if it differs from the input, a scale filter is inserted
   automatically.
3. `Bitrate` has `bps`, `kbps` and `mbps` constructors; `Bitrate::mbps(2)` = 2 Mbit/s.
4. `Framerate::fps(30)` is `30/1`. For fractional rates use `Framerate::ratio(30000, 1001)`
   (29.97 fps).
5. `preset` trades encoding speed for compression. Slower presets = smaller files.
6. `profile` constrains the H.264 feature set for device compatibility.
7. `build()` validates the config; a missing codec yields `Error::InvalidConfig`.
8. `.video(config)` attaches the video settings to the transcode.
9. `.drop_audio()` here means the result is video-only.

!!! warning "`preset` and `profile` are H.264/H.265 only"
    They map to `libx264`/`libx265` private options. When encoding VP9 or AV1, omit them —
    they are silently ignored for other codecs.

## Choosing other codecs

The container is inferred from the extension, so codec and container travel together:

```rust
use media::prelude::*;

let targets = [
    (VideoCodec::H265, "out.mp4"),   // (1)!
    (VideoCodec::Vp9,  "out.webm"),
    (VideoCodec::Av1,  "out.mkv"),
];

for (codec, output) in targets {
    let video = VideoConfig::builder()
        .codec(codec)
        .bitrate(Bitrate::mbps(1))   // (2)!
        .build()?;

    Transcoder::builder()
        .input("input.mp4")
        .output(output)
        .video(video)
        .drop_audio()
        .build()?
        .run()?;
}
# Ok::<(), media::Error>(())
```

1. `.mp4` → MP4, `.webm` → WebM, `.mkv` → Matroska. Pick an extension the codec fits.
2. Note there's no `preset`/`profile` here — those are H.264/H.265-specific (see the warning
   above).

The available codecs are `H264`, `H265`, `Vp8`, `Vp9`, `Av1` — see
[Types](../reference/types.md#videocodec).

## Dropping a stream

Keep only video, or only audio, with a single modifier on any tier:

```rust
use media::prelude::*;
# fn demo() -> media::Result<()> {
transcode("input.mp4").to("video_only.mp4").drop_audio()?; // ← won't compile; see note
# Ok(()) }
```

!!! danger "Remember `.run()`"
    The modifiers (`drop_audio`, `trim`, `video`, …) return the job — you still need `.run()`
    at the end. The correct forms:

    ```rust
    use media::prelude::*;
    # fn demo() -> media::Result<()> {
    transcode("input.mp4").to("video_only.mp4").drop_audio().run()?; // keep video
    transcode("input.mp4").to("audio_only.m4a").drop_video().run()?; // keep audio
    # Ok(()) }
    ```

`drop_video` keeps the audio stream and copies it through, which makes it an easy way to
extract an audio track.

## Trimming to a time range

Keep only part of the input. The range is an **inclusive** range of `Duration`s, and the
output is re-based to start at zero:

```rust
use media::prelude::*;
use std::time::Duration;

let summary = transcode("input.mp4")
    .to("clip.mp4")
    .trim(Duration::from_secs(1)..=Duration::from_secs(3)) // (1)!
    .run()?;
# Ok::<(), media::Error>(())
```

1. Keeps seconds 1–3 (inclusive). Under the hood the pipeline **seeks** to the keyframe at or
   before the start rather than decoding and discarding the prefix, so trimming deep into a
   long file is fast.

## Reporting progress

Swap `run()` for `run_with_progress` and pass a callback. It receives a
[`Progress`](../reference/transcode.md#progress) snapshot as the work advances:

```rust
use media::prelude::*;
use std::io::Write;

let job = Transcoder::builder()
    .input("input.mp4")
    .output("output.mp4")
    .drop_audio()
    .build()?;

let summary = job.run_with_progress(|p| { // (1)!
    print!(
        "\r{:5.1}%  {:.2}/{:.2}s  {} frames  {:.0} fps",
        p.percent(),          // (2)!
        p.processed_secs(),
        p.total_secs(),
        p.frames(),
        p.fps(),              // (3)!
    );
    let _ = std::io::stdout().flush();
})?;
println!("\nDone: {} frames", summary.frames);
# Ok::<(), media::Error>(())
```

1. The callback is `FnMut(Progress)`. It runs on the calling thread, so capturing and mutating
   local state is fine — no `Send`/`Sync` gymnastics.
2. `percent()` is clamped to `0..=100`; it returns `0.0` when the total duration is unknown.
3. `fps()` is the measured encoding throughput, not the video's frame rate.

## Applying filters

Attach a [`VideoFilterChain`](filters.md) with `.video_filter(...)` on any tier. See the
[Filters guide](filters.md) for the full builder.

```rust
use media::prelude::*;
# fn demo() -> media::Result<()> {
let chain = VideoFilterChain::new().scale(640, 360).fps(24);
transcode("input.mp4").to("output.mp4").drop_audio().video_filter(chain).run()?;
# Ok(()) }
```

## See also

- [Transcode API reference](../reference/transcode.md)
- [Filters](filters.md) · [Frame extraction](frame-extraction.md) (the symmetric API)
- [Low-level pipeline](low-level.md) — Tier 3, wired by hand
