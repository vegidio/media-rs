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
`video_filter`.

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
