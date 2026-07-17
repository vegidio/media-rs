# Frame extraction

Module `media::extract`. See the [Frame extraction guide](../guides/frame-extraction.md) for
walkthroughs.

## `extract_frames`

```rust
pub fn extract_frames(input: impl Into<String>) -> ExtractJob
```

Tier-1 entry point.

### `ExtractJob`

A fluent facade over [`FrameExtractorBuilder`](#frameextractor).

| Method | Signature | Description |
|--------|-----------|-------------|
| `every` | `every(interval: Duration) -> Self` | Sample one frame every _N_ (sugar for `Interval::EverySeconds`). |
| `interval` | `interval(interval: Interval) -> Self` | Any [`Interval`](#interval) sampling strategy. |
| `to_dir` | `to_dir(dir: impl Into<PathBuf>) -> Self` | Write image files into `dir`. |
| `run` | `run(self) -> Result<ExtractReport>` | Run to completion. |
| `run_with_progress` | `run_with_progress(self, on_progress: impl FnMut(Progress)) -> Result<ExtractReport>` | Run, reporting [progress](transcode.md#progress). |

## `FrameExtractor`

Tier-2 configured job.

```rust
pub fn builder() -> FrameExtractorBuilder
pub fn run(self) -> Result<ExtractReport>
pub fn run_with_progress(self, on_progress: impl FnMut(Progress)) -> Result<ExtractReport>
```

### `FrameExtractorBuilder`

| Method | Signature | Description |
|--------|-----------|-------------|
| `input` | `input(path: impl Into<String>) -> Self` | Input file (**required**). |
| `interval` | `interval(interval: Interval) -> Self` | Sampling strategy (**required**). |
| `format` | `format(format: ImageFormat) -> Self` | Output image format (default JPEG q90). |
| `resolution` | `resolution(resolution: Resolution) -> Self` | Output resolution (default `Original`). |
| `naming` | `naming(naming: NamingScheme) -> Self` | File naming (directory output only). |
| `range` | `range(range: RangeInclusive<Duration>) -> Self` | Restrict to a time span. |
| `output` | `output(output: Output) -> Self` | Set the output target explicitly. |
| `output_dir` | `output_dir(dir: impl Into<PathBuf>) -> Self` | Shortcut for `Output::Directory`. |
| `to_memory` | `to_memory() -> Self` | Shortcut for `Output::InMemory`. |
| `to_callback` | `to_callback(f: impl FnMut(ExtractedFrame) -> Result<()> + 'static) -> Self` | Shortcut for `Output::Callback`. |
| `build` | `build(self) -> Result<FrameExtractor>` | Validate required fields. |

`build` requires `input`, `interval`, and an output; otherwise
[`Error::InvalidConfig`](errors.md).

## `Interval`

Which frames to extract.

```rust
pub enum Interval {
    EverySeconds(f64),        // 0s, N, 2N, … (a rate in seconds)
    EveryNFrames(u32),        // every N-th decoded frame: 0, N, 2N, …
    Fps(f64),                 // N frames/sec; == EverySeconds(1.0 / fps)
    Count(u32),               // exactly N, evenly spaced across the duration
    Timestamps(Vec<Duration>),// frames at these exact times
}
```

## `ImageFormat`

```rust
pub enum ImageFormat {
    Jpeg { quality: u8 },  // 1 (smallest) – 100 (best)
    Png,                   // lossless
}
```

Default: `Jpeg { quality: 90 }`.

| Method | Signature | Description |
|--------|-----------|-------------|
| `extension` | `extension(self) -> &'static str` | `"jpg"` or `"png"` (no dot). |
| `from_extension` | `from_extension(ext: &str) -> Option<Self>` | Infer from an extension. |
| `from_path` | `from_path(path: impl AsRef<Path>) -> Option<Self>` | Infer from a path's extension. |

## `Resolution`

```rust
pub enum Resolution {
    Original,             // keep source dimensions (default)
    Fixed(u32, u32),      // scale every frame to width × height
}
```

## `NamingScheme`

```rust
pub enum NamingScheme {
    Sequential { prefix: String, padding: usize }, // <prefix>_<index>.<ext>, zero-padded
}
```

Default: `Sequential { prefix: "frame", padding: 4 }` → `frame_0000.jpg`.

## `Output`

```rust
pub enum Output {
    Directory(PathBuf),                                   // write image files
    InMemory,                                             // buffer in RAM (see ExtractReport::frames)
    Callback(Box<dyn FnMut(ExtractedFrame) -> Result<()>>), // stream to a closure
}
```

## `ExtractedFrame`

One extracted frame, carrying packed RGB pixels, its run index, and its source timestamp.

| Method | Returns | Description |
|--------|---------|-------------|
| `index` | `u32` | Zero-based position in the run. |
| `timestamp` | `Duration` | When it occurs in the source. |
| `dimensions` | `(u32, u32)` | `(width, height)` in pixels. |
| `to_rgb_bytes` | `&[u8]` | Tightly-packed RGB (8:8:8), `width * height * 3` bytes. |
| `encode` | `encode(&self, format: ImageFormat) -> Result<Vec<u8>>` | Encode to image bytes in memory. |
| `save` | `save(&self, path: impl AsRef<Path>) -> Result<()>` | Write to a file; format from the extension. |

## `ExtractReport`

What a run produced.

| Method | Returns | Description |
|--------|---------|-------------|
| `frame_count` | `u64` | How many frames were extracted. |
| `elapsed` | `Duration` | Wall-clock time the run took. |
| `frames` | `&[ExtractedFrame]` | Buffered frames (`InMemory` only; empty otherwise). |
| `into_frames` | `Vec<ExtractedFrame>` | Take ownership of the buffered frames. |

## `SampledFrames`

Tier-3 iterator returned by
[`StreamRef::sampled_at`](format.md#streamref). Implements
`Iterator<Item = Result<ExtractedFrame>>`, seeking and decoding lazily at full source
resolution.
