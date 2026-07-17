# Quick start

Every example on this site starts with one import:

```rust
use media::prelude::*;
```

The [prelude](../reference/index.md) re-exports the common entry points (`transcode`,
`extract_frames`, `probe`, `MediaReader`, …) and the typed building blocks
(`VideoCodec`, `Bitrate`, `Framerate`, …), so you rarely need to reach into submodules.

## Inspect a file

Before touching pixels, see what a file contains — this decodes nothing:

```rust
use media::prelude::*;

let info = probe("input.mp4")?; // (1)!
println!("{:.2}s, {} streams", info.duration().as_secs_f64(), info.stream_count());

if let Some(v) = info.video() { // (2)!
    println!("video: {}x{} {:?}", v.width, v.height, v.video_codec);
}
# Ok::<(), media::Error>(())
```

1. `probe` opens the container and reads its metadata. It returns a
   [`MediaInfo`](../reference/probe.md), never decoding a frame.
2. `video()` returns the first video stream as an `Option<&StreamInfo>`; `audio()` does the
   same for audio.

## Transcode a file

The one-liner infers the container and codecs from the file extensions:

```rust
use media::prelude::*;

let summary = transcode("input.mp4").to("output.mp4").run()?; // (1)!
println!("{} frames, {:.2}s", summary.frames, summary.duration_secs); // (2)!
# Ok::<(), media::Error>(())
```

1. `transcode(input)` starts a job, `.to(output)` sets the destination, and `.run()`
   executes it. That is the whole happy path.
2. `run()` returns a [`TranscodeSummary`](../reference/transcode.md) with the frame count and
   output duration.

## Extract still images

Save one JPEG per second into a directory:

```rust
use media::prelude::*;
use std::time::Duration;

let report = extract_frames("input.mp4") // (1)!
    .every(Duration::from_secs(1)) // (2)!
    .to_dir("frames/") // (3)!
    .run()?;

println!("wrote {} frames in {:?}", report.frame_count(), report.elapsed());
# Ok::<(), media::Error>(())
```

1. `extract_frames` mirrors `transcode`: same fluent shape, same three tiers.
2. `.every(Duration)` samples one frame every _N_ seconds. Sampling is **seek-based**, so
   this stays fast even on a long video.
3. `.to_dir` writes image files (JPEG by default), creating the directory if needed.

## Where to go next

- [Core concepts](concepts.md) — the three-tier API, time types, progress and logging.
- [Guides](../guides/index.md) — a focused, richly-explained walkthrough per feature.
- [API reference](../reference/index.md) — every public type and method.
