# API reference

Hand-written reference for the public API. For a task-oriented introduction, start with the
[Guides](../guides/index.md).

## The prelude

`use media::prelude::*;` re-exports everything below in one import. Prefer it over reaching
into submodules.

```rust
use media::prelude::*;
```

It brings in:

- **Errors:** [`Error`](errors.md), [`Result`](errors.md)
- **Transcode:** [`transcode`](transcode.md), [`Transcoder`](transcode.md#transcoder),
  [`VideoConfig`](transcode.md#videoconfig), [`Progress`](transcode.md#progress),
  [`TranscodeSummary`](transcode.md#transcodesummary)
- **Extract:** [`extract_frames`](extract.md), [`FrameExtractor`](extract.md#frameextractor),
  [`Interval`](extract.md#interval), [`ImageFormat`](extract.md#imageformat),
  [`Resolution`](extract.md#resolution), [`NamingScheme`](extract.md#namingscheme),
  [`Output`](extract.md#output), [`ExtractedFrame`](extract.md#extractedframe),
  [`ExtractReport`](extract.md#extractreport), [`SampledFrames`](extract.md#sampledframes)
- **Format I/O:** [`MediaReader`](format.md#mediareader), [`MediaWriter`](format.md#mediawriter)
- **Codec:** [`Decoder`](codec.md#decoder), [`VideoEncoder`](codec.md#videoencoder)
- **Filters:** [`FilterChain`](filter.md#filterchain), [`DenoiseLevel`](filter.md#denoiselevel),
  [`ColorCorrect`](filter.md#colorcorrect)
- **Probe:** [`probe`](probe.md), [`MediaInfo`](probe.md#mediainfo),
  [`StreamInfo`](probe.md#streaminfo)
- **Frame & Packet:** [`Frame`](frame-packet.md#frame), [`Packet`](frame-packet.md#packet)
- **Logging:** [`log`](logging.md) module, [`Level`](logging.md#level)
- **Types:** [`VideoCodec`](types.md#videocodec), [`AudioCodec`](types.md#audiocodec),
  [`Bitrate`](types.md#bitrate), [`Framerate`](types.md#framerate),
  [`Rational`](types.md#rational), [`H264Preset`](types.md#h264preset),
  [`H264Profile`](types.md#h264profile), [`PixelFormat`](types.md#pixelformat),
  [`SampleFormat`](types.md#sampleformat), [`SampleRate`](types.md#samplerate),
  [`Channels`](types.md#channels), [`StreamKind`](types.md#streamkind)

## Module map

| Module | Contents | Reference |
|--------|----------|-----------|
| `media::transcode` | Transcoding API | [Transcode](transcode.md) |
| `media::extract` | Frame extraction | [Frame extraction](extract.md) |
| `media::format` | `MediaReader` / `MediaWriter` | [Format I/O](format.md) |
| `media::codec` | `Decoder` / `VideoEncoder` | [Codec](codec.md) |
| `media::frame`, `media::packet` | `Frame`, `Packet` | [Frame & Packet](frame-packet.md) |
| `media::filter` | `FilterChain` and friends | [Filters](filter.md) |
| `media::probe` | `probe`, `MediaInfo` | [Probe](probe.md) |
| `media::types` | Codecs, rates, formats | [Types](types.md) |
| `media::log` | Log verbosity | [Logging](logging.md) |
| `media::error` | `Error`, `Result` | [Errors](errors.md) |
| `media::sys` | Raw FFI bindings (`unsafe`) | [Raw FFI guide](../guides/raw-ffi.md) |

## Crate-level function

```rust
pub fn version_info() -> &'static str
```

Returns the build version string of the linked FFmpeg libraries (e.g. `"8.1.2"`). Useful as a
smoke test that the static libraries linked and are callable.

## Conventions

- **Builders** follow `X::builder()` → chained by-value `self` setters → `.build() -> Result`
  (validating required fields, else [`Error::InvalidConfig`](errors.md)).
- **One-liner facades** (`XJob`) are thin wrappers over the corresponding builder.
- **Fallible calls** return [`Result<T>`](errors.md) = `Result<T, media::Error>`.
