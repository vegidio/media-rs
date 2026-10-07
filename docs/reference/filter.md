# Filters

Module `media::filter`. Composable, typed video filters. See the
[Filters guide](../guides/filters.md).

## `VideoFilterChain`

A chain of video filters. Empty by default; each operator appends a stage, and stages run in
the order added.

| Method | Signature | Description |
|--------|-----------|-------------|
| `new` | `new() -> Self` | An empty chain (a no-op). |
| `raw` | `raw(description: impl Into<String>) -> Self` | A chain from a raw libavfilter string, used verbatim. |
| `scale` | `scale(width: u32, height: u32) -> Self` | Scale to `width`×`height`. |
| `fps` | `fps(fps: u32) -> Self` | Force a constant frame rate. |
| `denoise` | `denoise(level: DenoiseLevel) -> Self` | Denoise at the given strength. |
| `color_correct` | `color_correct(f: impl FnOnce(ColorCorrect) -> ColorCorrect) -> Self` | Color-correct via a closure. |
| `is_empty` | `is_empty(&self) -> bool` | `true` if no stages were added. |
| `description` | `description(&self) -> String` | The combined libavfilter string (stages joined with `,`). |

Apply a chain with [`transcode(...).video_filter(chain)`](transcode.md) or the builder's
`video_filter`, or run it yourself on decoded frames with a [`VideoFilter`](#videofilter).

## `VideoFilter`

A `VideoFilterChain`, built for the frames of one decoder and ready to run. Use it in a
[low-level pipeline](../guides/low-level.md) to scale, turn or convert frames before you encode
them. Its audio counterpart is [`AudioFilter`](audio.md#audiofilter).

| Method | Signature | Description |
|--------|-----------|-------------|
| `new` | `new(decoder: &Decoder, time_base: Rational, chain: &VideoFilterChain) -> Result<Self>` | Build `chain` for `decoder`'s frames (their size, pixel format and sample aspect ratio), whose timestamps are in `time_base`. An empty chain passes frames through. |
| `filter` | `filter(&mut self, frame: Frame) -> Result<Vec<Frame>>` | Push one frame through; returns every frame that comes out (usually one). |
| `flush` | `flush(&mut self) -> Result<Vec<Frame>>` | End of stream: returns the frames the chain was still holding. |
| `output_width` | `output_width(&self) -> u32` | Width of the frames it emits. |
| `output_height` | `output_height(&self) -> u32` | Height of the frames it emits. |
| `output_pixel_format` | `output_pixel_format(&self) -> PixelFormat` | Pixel format of the frames it emits. |
| `output_time_base` | `output_time_base(&self) -> Rational` | Time base of the timestamps it emits; most stages keep the input's, `fps` doesn't. |
| `output_frame_rate` | `output_frame_rate(&self) -> Option<Framerate>` | Frame rate of the frames it emits, when the chain knows it (after `fps`). |

The `output_*` accessors answer before any frame goes in, so you can size a
[`VideoEncoder`](codec.md#videoencoder) from them. The input shape is fixed when the filter is
built: a stream that changes resolution midway needs a new filter.

## `DenoiseLevel`

```rust
pub enum DenoiseLevel {
    Light,     // subtle
    Moderate,  // balanced
    Heavy,     // strong
}
```

Each maps to a tuned `hqdn3d` setting.

## `ColorCorrect`

Color adjustment knobs (applied via the `eq` filter). Built with a closure inside
`VideoFilterChain::color_correct`. Each setter returns `Self`; only the knobs you touch move away
from their identity defaults.

| Method | Identity | Description |
|--------|:--------:|-------------|
| `brightness` | `0.0` | Brightness shift, roughly `[-1.0, 1.0]`. |
| `contrast` | `1.0` | Contrast multiplier. |
| `saturation` | `1.0` | Saturation multiplier (`0.0` = grayscale). |
| `gamma` | `1.0` | Gamma. |

```rust
use media::prelude::*;
let chain = VideoFilterChain::new()
    .color_correct(|cc| cc.brightness(0.05).contrast(1.1).saturation(1.2));
# let _ = chain;
```
