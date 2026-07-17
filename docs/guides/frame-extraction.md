# Frame extraction

**Use when:** you want still images out of a video — thumbnails, a contact sheet, frames for
ML, a scrubbing preview. The API mirrors [transcoding](transcoding.md): a one-liner, a
builder, and a Tier-3 iterator.

!!! tip "Sampling is seek-based"
    Extraction seeks to each sample point and decodes only what it needs. Pulling one frame
    every 10 seconds from a two-hour video does **not** decode the whole stream.

## Tier 1 — one JPEG per second to a folder

```rust
use media::prelude::*;
use std::time::Duration;

let report = extract_frames("input.mp4") // (1)!
    .every(Duration::from_secs(1))       // (2)!
    .to_dir("frames/")                   // (3)!
    .run()?;

println!("wrote {} frames in {:?}", report.frame_count(), report.elapsed()); // (4)!
# Ok::<(), media::Error>(())
```

1. `extract_frames(input)` starts the one-liner job.
2. `.every(Duration)` is shorthand for `Interval::EverySeconds`. It samples at 0s, 1s, 2s, …
3. `.to_dir(dir)` writes image files there (JPEG at quality 90 by default), creating the
   directory if it's missing. Files are named `frame_0000.jpg`, `frame_0001.jpg`, …
4. Every tier returns an [`ExtractReport`](../reference/extract.md#extractreport):
   `frame_count()` and `elapsed()`.

## Tier 2 — the builder

The builder exposes every knob: interval, format, resolution, naming, a time range, and where
frames go.

=== "To memory"

    ```rust
    use media::prelude::*;

    let report = FrameExtractor::builder()
        .input("input.mp4")
        .interval(Interval::Count(5))          // (1)!
        .format(ImageFormat::Png)              // (2)!
        .resolution(Resolution::Fixed(320, 180)) // (3)!
        .to_memory()                           // (4)!
        .build()?
        .run()?;

    for frame in report.frames() {             // (5)!
        let (w, h) = frame.dimensions();
        println!("frame {} @ {:?} — {w}x{h}", frame.index(), frame.timestamp());
    }
    # Ok::<(), media::Error>(())
    ```

    1. `Interval::Count(5)` extracts **exactly 5** frames, evenly spread across the duration.
    2. `ImageFormat::Png` (lossless) or `ImageFormat::Jpeg { quality }`.
    3. `Resolution::Fixed(w, h)` scales every frame; the default is `Resolution::Original`.
    4. `.to_memory()` keeps frames in RAM instead of writing files.
    5. `report.frames()` returns them as `&[ExtractedFrame]`. `frames()` is empty for the
       directory and callback outputs, which don't buffer.

=== "To a callback"

    ```rust
    use media::prelude::*;
    use std::time::Duration;

    FrameExtractor::builder()
        .input("input.mp4")
        .interval(Interval::Timestamps(vec![       // (1)!
            Duration::from_millis(500),
            Duration::from_millis(1_500),
        ]))
        .to_callback(|frame| {                      // (2)!
            let jpeg = frame.encode(ImageFormat::Jpeg { quality: 85 })?; // (3)!
            println!("frame {} → {} bytes", frame.index(), jpeg.len());
            Ok(())                                  // (4)!
        })
        .build()?
        .run()?;
    # Ok::<(), media::Error>(())
    ```

    1. `Interval::Timestamps` grabs frames at exact moments you name.
    2. `.to_callback(f)` streams each frame to your closure as it's produced — nothing is
       buffered. The closure is `FnMut(ExtractedFrame) -> Result<()>` and must be `'static`.
    3. `encode` returns the image as a `Vec<u8>` in the format you ask for, without touching
       disk.
    4. Returning `Err(...)` from the callback aborts the extraction and propagates the error.

=== "With a range + progress"

    ```rust
    use media::prelude::*;
    use std::time::Duration;

    FrameExtractor::builder()
        .input("input.mp4")
        .interval(Interval::EverySeconds(0.5))                       // (1)!
        .range(Duration::from_secs(1)..=Duration::from_secs(2))      // (2)!
        .to_memory()
        .build()?
        .run_with_progress(|p| {                                     // (3)!
            println!("{} frames ({:.0}%)", p.frames(), p.percent());
        })?;
    # Ok::<(), media::Error>(())
    ```

    1. `EverySeconds(0.5)` samples twice per second (a rate, so it's an `f64`).
    2. `.range(start..=end)` restricts extraction to a span of the source.
    3. `run_with_progress` reuses the same [`Progress`](../reference/transcode.md#progress)
       type as the transcoder.

## Sampling strategies

The [`Interval`](../reference/extract.md#interval) enum picks which frames you get:

| Variant | Picks | Notes |
|---------|-------|-------|
| `EverySeconds(f64)` | 0s, N, 2N, … | A rate in seconds. `.every(Duration)` is sugar for this. |
| `Fps(f64)` | N frames per second | Equivalent to `EverySeconds(1.0 / fps)`. |
| `EveryNFrames(u32)` | frames 0, N, 2N, … | Counts **decoded** frames, decoded sequentially. |
| `Count(u32)` | exactly N | Evenly spaced across the (trimmed) duration. |
| `Timestamps(Vec<Duration>)` | those exact times | Arbitrary moments. |

```rust
use media::prelude::*;
# fn demo() -> media::Result<()> {
// Fixed 2 fps regardless of the source rate:
let a = FrameExtractor::builder()
    .input("input.mp4").interval(Interval::Fps(2.0)).to_memory().build()?.run()?;

// Every 15th decoded frame:
let b = FrameExtractor::builder()
    .input("input.mp4").interval(Interval::EveryNFrames(15)).to_memory().build()?.run()?;
# let _ = (a, b);
# Ok(()) }
```

## Custom file names

When writing to a directory, override the default `frame_0000` naming:

```rust
use media::prelude::*;
# fn demo() -> media::Result<()> {
FrameExtractor::builder()
    .input("input.mp4")
    .interval(Interval::Count(3))
    .naming(NamingScheme::Sequential { prefix: "shot".into(), padding: 3 }) // (1)!
    .output_dir("frames/")                                                  // (2)!
    .build()?
    .run()?;
# Ok(()) }
```

1. Produces `shot_000.jpg`, `shot_001.jpg`, … — `padding` is the zero-pad width. The default
   scheme is `frame` with padding `4`.
2. `.output_dir(dir)` is equivalent to `.output(Output::Directory(dir.into()))`.

## Working with an `ExtractedFrame`

Whether from `report.frames()` or a callback, each
[`ExtractedFrame`](../reference/extract.md#extractedframe) carries packed RGB pixels plus its
index and timestamp:

```rust
use media::prelude::*;
# fn demo(report: media::ExtractReport) -> media::Result<()> {
for frame in report.into_frames() {         // (1)!
    let (w, h) = frame.dimensions();
    let bytes = frame.to_rgb_bytes();       // (2)!  &[u8], w*h*3 bytes, row-major RGB
    frame.save(format!("frame_{}.png", frame.index()))?; // (3)!
    let _ = (w, h, bytes);
}
# Ok(()) }
```

1. `into_frames()` takes ownership of the buffered frames (vs `frames()` which borrows).
2. `to_rgb_bytes()` borrows the raw, tightly-packed RGB (8:8:8) buffer — `width * height * 3`
   bytes.
3. `save(path)` writes the frame, choosing the encoder from the file extension. There's also
   `encode(format)` if you want the bytes in memory.

## Tier 3 — iterate frames yourself

For full control, sample a stream directly and get an iterator of
`Result<ExtractedFrame>`:

```rust
use media::prelude::*;

let mut reader = MediaReader::open("input.mp4")?;
let vidx = reader.best_stream(StreamKind::Video)?;

for frame in reader.stream(vidx).sampled_at(Interval::Count(3))? { // (1)!
    let frame = frame?;                                            // (2)!
    let (w, h) = frame.dimensions();
    println!("frame {} is {w}x{h}", frame.index());
}
# Ok::<(), media::Error>(())
```

1. `stream(idx).sampled_at(interval)` returns a [`SampledFrames`](../reference/extract.md#sampledframes)
   iterator. It seeks and decodes lazily, at the source's full resolution.
2. Each item is a `Result` — an I/O or decode error surfaces here.

## See also

- [Frame extraction API reference](../reference/extract.md)
- [Seeking](seeking.md) — the seek primitive extraction is built on
