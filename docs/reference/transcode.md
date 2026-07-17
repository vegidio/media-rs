# Transcode

Module `media::transcode`. See the [Transcoding guide](../guides/transcoding.md) for
walkthroughs.

## `transcode`

```rust
pub fn transcode(input: impl Into<String>) -> TranscodeJob
```

Tier-1 entry point. Begins a one-liner transcode from `input`.

### `TranscodeJob`

A fluent facade over [`TranscoderBuilder`](#transcoder). Chain modifiers, then `run`.

| Method | Signature | Description |
|--------|-----------|-------------|
| `to` | `to(output: impl Into<String>) -> Self` | Output file (required); container inferred from the extension. |
| `video` | `video(config: VideoConfig) -> Self` | Override video encoding settings. |
| `video_filter` | `video_filter(filter: FilterChain) -> Self` | Apply a [filter chain](filter.md). |
| `drop_video` | `drop_video() -> Self` | Drop the video stream. |
| `drop_audio` | `drop_audio() -> Self` | Drop the audio stream. |
| `trim` | `trim(range: RangeInclusive<Duration>) -> Self` | Keep only this time range. |
| `run` | `run(self) -> Result<TranscodeSummary>` | Run to completion. |
| `run_with_progress` | `run_with_progress(self, on_progress: impl FnMut(Progress)) -> Result<TranscodeSummary>` | Run, reporting [progress](#progress). |

## `Transcoder`

Tier-2 configured job. Cheap to hold and **re-runnable** (`run` takes `&self`).

```rust
pub fn builder() -> TranscoderBuilder
pub fn run(&self) -> Result<TranscodeSummary>
pub fn run_with_progress(&self, on_progress: impl FnMut(Progress)) -> Result<TranscodeSummary>
```

### `TranscoderBuilder`

| Method | Signature | Description |
|--------|-----------|-------------|
| `input` | `input(path: impl Into<String>) -> Self` | Input file (**required**). |
| `output` | `output(path: impl Into<String>) -> Self` | Output file (**required**). |
| `video` | `video(config: VideoConfig) -> Self` | Video encoding config. Omit to inherit the input's geometry with H.264. |
| `video_filter` | `video_filter(filter: FilterChain) -> Self` | Filter chain. |
| `drop_video` | `drop_video() -> Self` | Drop video. |
| `drop_audio` | `drop_audio() -> Self` | Drop audio. |
| `trim` | `trim(range: RangeInclusive<Duration>) -> Self` | Keep only this range, re-based to start at zero. |
| `build` | `build(self) -> Result<Transcoder>` | Validate; missing input/output → [`Error::InvalidConfig`](errors.md). |

## `VideoConfig`

How to encode the output video stream. Anything unset is inherited from the input.

```rust
pub fn builder() -> VideoConfigBuilder
```

### `VideoConfigBuilder`

| Method | Signature | Description |
|--------|-----------|-------------|
| `codec` | `codec(codec: VideoCodec) -> Self` | The video codec (**required**). |
| `resolution` | `resolution(width: u32, height: u32) -> Self` | Output size; inserts a scale filter if it differs from the input. |
| `bitrate` | `bitrate(bitrate: Bitrate) -> Self` | Target bit rate. |
| `framerate` | `framerate(framerate: Framerate) -> Self` | Output frame rate. |
| `preset` | `preset(preset: H264Preset) -> Self` | Speed/quality preset (H.264/H.265 only). |
| `profile` | `profile(profile: H264Profile) -> Self` | Codec profile (H.264/H.265 only). |
| `build` | `build(self) -> Result<VideoConfig>` | Validate; missing codec → [`Error::InvalidConfig`](errors.md). |

See [Types](types.md) for `VideoCodec`, `Bitrate`, `Framerate`, `H264Preset`, `H264Profile`.

## `Progress`

An immutable snapshot passed to `run_with_progress` callbacks. Shared with
[frame extraction](extract.md).

| Method | Returns | Description |
|--------|---------|-------------|
| `percent` | `f64` | Completion in `0..=100` (`0.0` if total unknown). |
| `processed_secs` | `f64` | Seconds of media processed so far. |
| `total_secs` | `f64` | Total media duration (`0.0` if unknown). |
| `frames` | `u64` | Frames encoded so far. |
| `fps` | `f64` | Encoding throughput (frames/sec), not the video's frame rate. |

## `TranscodeSummary`

The outcome of a completed transcode. Both fields are public:

```rust
pub struct TranscodeSummary {
    pub frames: u64,        // total video frames encoded
    pub duration_secs: f64, // output media duration in seconds (best-effort)
}
```
