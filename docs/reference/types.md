# Types

Module `media::types`. Strongly-typed building blocks shared across the API. All are
re-exported by the prelude.

## `VideoCodec`

```rust
pub enum VideoCodec { H264, H265, Vp8, Vp9, Av1 }
```

Encoders resolve **by name** to the external libraries.

| Variant | Encoder      | Common container |
|---------|--------------|------------------|
| `H264`  | `libx264`    | `.mp4`           |
| `H265`  | `libx265`    | `.mp4`           |
| `Vp8`   | `libvpx`     | `.webm`          |
| `Vp9`   | `libvpx-vp9` | `.webm`          |
| `Av1`   | `libsvtav1`  | `.mkv`           |

Methods: `encoder_name(self) -> &'static str`, `codec_id(self) -> sys::AVCodecID`.

## `AudioCodec`

```rust
pub enum AudioCodec { Aac, Mp3, Opus, Vorbis, Flac }
```

Methods: `encoder_name`, `codec_id`.

!!! note "Used for audio encoding"
    These types drive the [audio encoding API](../guides/audio.md) (`AudioConfig`,
    `AudioEncoder`, `AudioFilterChain`, `Resampler`) as well as [probe](probe.md) results.
    Transcoding still **copies** audio by default and re-encodes only when asked.

## `H264Preset`

```rust
pub enum H264Preset {
    Ultrafast, Superfast, Veryfast, Faster, Fast, Medium, Slow, Slower, Veryslow,
}
```

Speed↔compression trade-off for x264/x265. Slower presets = smaller files at the same quality.
`Medium` is the encoder default. Method: `as_str(self) -> &'static str`.

## `H264Profile`

```rust
pub enum H264Profile { Baseline, Main, High }
```

Constrains the H.264 feature set for device compatibility. `Baseline` = widest compatibility;
`High` = best compression (the common SD/HD default). Method: `as_str`.

## `Rational`

An exact rational `num / den`, mirroring `AVRational`. FFmpeg uses rationals for time bases and
frame rates so exact values survive arithmetic without float drift.

```rust
pub struct Rational { pub num: i32, pub den: i32 }
```

| Item | Signature | Description |
|------|-----------|-------------|
| `ONE` | `const Rational` | `1/1` — the canonical unit time base / square SAR. |
| `new` | `const fn new(num: i32, den: i32) -> Self` | Construct. |
| `as_f64` | `as_f64(self) -> f64` | Evaluate (`0.0` for a zero denominator). |

## `Framerate`

```rust
pub struct Framerate(pub Rational);
```

| Item | Signature | Description |
|------|-----------|-------------|
| `fps` | `const fn fps(fps: u32) -> Self` | Integer rate, e.g. `Framerate::fps(30)` → `30/1`. |
| `ratio` | `const fn ratio(num: i32, den: i32) -> Self` | Rational rate, e.g. `ratio(30000, 1001)` (29.97). |
| `as_f64` | `as_f64(self) -> f64` | The rate as a float. |

## `Bitrate`

A bit rate in bits per second.

```rust
pub struct Bitrate(pub i64);
```

| Item | Signature | Description |
|------|-----------|-------------|
| `bps` | `const fn bps(bps: i64) -> Self` | From bits/sec. |
| `kbps` | `const fn kbps(kbps: i64) -> Self` | From kbit/sec (×1000). |
| `mbps` | `const fn mbps(mbps: i64) -> Self` | From Mbit/sec (×1,000,000). |
| `as_bps` | `const fn as_bps(self) -> i64` | The value in bits/sec. |

## `PixelFormat`

```rust
pub enum PixelFormat {
    Yuv420p, Yuv422p, Yuv444p, // planar YUV
    Nv12,                       // semi-planar YUV 4:2:0
    Rgb24, Rgba,                // packed RGB
    Other(i32),                 // any other format; carries the raw id
}
```

A curated subset of FFmpeg's `AVPixelFormat`. Formats not listed round-trip through `Other`.

## `SampleFormat`

```rust
pub enum SampleFormat {
    U8, S16, S32, Flt, Dbl,       // interleaved
    S16p, S32p, Fltp, Dblp,       // planar (suffix `p`)
    Other(i32),
}
```

Variants ending in `p` are planar (one buffer per channel); the others are interleaved.

## `SampleRate`

```rust
pub enum SampleRate {
    Hz8000, Hz16000, Hz22050, Hz44100, Hz48000, Hz96000,
    Hz(u32), // arbitrary
}
```

Method: `hz(self) -> i32`.

## `Channels`

```rust
pub enum Channels { Mono, Stereo, Count(u32) }
```

Method: `count(self) -> u32`.

## `StreamKind`

```rust
pub enum StreamKind { Video, Audio, Subtitle, Data, Other }
```

The kind of data a stream carries; returned by [`probe`](probe.md) and passed to
[`MediaReader::best_stream`](format.md#mediareader).
