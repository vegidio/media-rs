# Codec

Module `media::codec`. Frame-level decoding and encoding. See the
[Low-level pipeline guide](../guides/low-level.md).

This page covers video decoding (`Decoder`) and video encoding (`VideoEncoder`). The audio
encoders — `AudioEncoder` and `Resampler`, also exported from `media::codec` — are documented
in the [Audio reference](audio.md).

## `Decoder`

A decoder bound to one input stream. Build one with
[`StreamRef::decoder`](format.md#streamref). Feed it packets, drain the returned iterator, then
`flush` at end of input.

| Method | Signature | Description |
|--------|-----------|-------------|
| `decode` | `decode(&mut self, packet: &Packet) -> Result<DecodeIter<'_>>` | Submit a packet; returns an iterator over the frames it produces. |
| `flush` | `flush(&mut self) -> Result<DecodeIter<'_>>` | Drain buffered frames at end of input. |
| `width` | `width(&self) -> u32` | Decoded frame width. |
| `height` | `height(&self) -> u32` | Decoded frame height. |
| `pixel_format` | `pixel_format(&self) -> PixelFormat` | Output pixel format. |
| `sample_aspect_ratio` | `sample_aspect_ratio(&self) -> Rational` | The shape of a pixel, width to height. `1:1` for square pixels and for a stream that doesn't say; something else, such as `4:3`, for an anamorphic one. Multiply the width by it for the shape to show the picture at. |
| `reset` | `reset(&mut self)` | Discard buffered state; **call after seeking** the reader. |

!!! warning "Drain fully, then send"
    The iterator from `decode`/`flush` borrows the decoder mutably, so drain it (or drop it)
    before the next `decode`. This matches FFmpeg's contract: receive all output before
    sending more input.

### `DecodeIter`

`Iterator<Item = Result<Frame>>` over the frames from one `decode`/`flush` call. Decoding is
lazy — iterate to actually receive frames.

## `VideoEncoder`

A configured, opened video encoder. Build with `VideoEncoder::builder()`.

| Method | Signature | Description |
|--------|-----------|-------------|
| `builder` | `builder() -> VideoEncoderBuilder` | Start configuring. |
| `encode` | `encode(&mut self, frame: &Frame) -> Result<EncodeIter<'_>>` | Submit a frame (PTS must be in the encoder's time base). Keyframes are placed by the encoder's own settings, never by the source's frame types. |
| `flush` | `flush(&mut self) -> Result<EncodeIter<'_>>` | Drain buffered packets at end of stream. |
| `time_base` | `time_base(&self) -> Rational` | The encoder's time base. |

!!! danger "Flush or truncate"
    At end of stream you **must** drain `flush()` — the encoder may hold a whole GOP. Skipping
    it silently truncates the output.

### `VideoEncoderBuilder`

| Method | Signature | Description |
|--------|-----------|-------------|
| `codec` | `codec(codec: VideoCodec) -> Self` | Codec (**required**). |
| `encoder` | `encoder(name: impl Into<String>) -> Self` | Encode with the FFmpeg encoder of that name instead of the codec's own, such as `h264_videotoolbox`. See [Hardware encoders](../guides/hardware-encoders.md). |
| `resolution` | `resolution(width: u32, height: u32) -> Self` | Output size (**required** unless `from_decoder`). |
| `from_decoder` | `from_decoder(decoder: &Decoder) -> Self` | Inherit resolution, pixel format and frame rate. |
| `pixel_format` | `pixel_format(pix_fmt: PixelFormat) -> Self` | Pixel format (default: decoder's, else YUV420p). |
| `framerate` | `framerate(framerate: Framerate) -> Self` | Output frame rate (default 25). |
| `time_base` | `time_base(time_base: Rational) -> Self` | Time base for incoming frame timestamps. |
| `bitrate` | `bitrate(bitrate: Bitrate) -> Self` | Target bit rate. |
| `preset` | `preset(preset: H264Preset) -> Self` | Speed/quality preset (H.264/H.265), for an encoder that has a `preset` of its own. |
| `profile` | `profile(profile: H264Profile) -> Self` | Codec profile (H.264/H.265), for an encoder that has a `profile` of its own. |
| `gop_size` | `gop_size(gop_size: u32) -> Self` | Keyframe interval (default 12). |
| `global_header` | `global_header(enabled: bool) -> Self` | Global-header flag (**on by default**; needed for MP4/MKV/WebM). |
| `option` | `option(key: impl AsRef<str>, value: impl AsRef<str>) -> Self` | Any encoder option by FFmpeg's name: a generic one such as `bf` or `sc_threshold`, or one of the encoder's own, such as libx264's `tune`. Repeatable; a later value for a key wins, and an option wins over the typed setter for the same setting. |
| `build` | `build(self) -> Result<VideoEncoder>` | Validate and open the encoder. |
| `probe` | `probe(self, frames: u32) -> Result<Vec<Packet>>` | Build the encoder, encode `frames` generated frames with a hard scene cut halfway, flush, and return the packets: whether this configuration encodes on this machine. |

The builder is `Clone`, so you can `probe` one copy and `build` the other.

`build` requires a codec and a resolution; a zero/out-of-range resolution →
[`Error::UnsupportedResolution`](errors.md), a missing codec/resolution →
[`Error::InvalidConfig`](errors.md), and an `option` the encoder doesn't recognise →
[`Error::UnknownOption`](errors.md), so a typo is never silently ignored. An `encoder` missing
from this FFmpeg build → [`Error::CodecUnavailable`](errors.md), and one for a different codec →
[`Error::InvalidConfig`](errors.md). A typed `preset` or `profile` on an encoder that has no
option of that name (VideoToolbox has no `preset`) → [`Error::UnknownOption`](errors.md).

!!! note "Keyframes are the encoder's choice"
    A decoded frame remembers whether it was an I, P or B frame in its source, and some
    encoders (libx264 among them) would take that as an order. `encode` clears it, so a
    re-encode places keyframes by `gop_size` and its own scene-cut detection only, as the
    `ffmpeg` command line does. To get exactly one keyframe at the start of a run of frames,
    use a `gop_size` above their count with `option("sc_threshold", "0")`.

### `EncodeIter`

`Iterator<Item = Result<Packet>>` over the packets from one `encode`/`flush` call. Encoding is
lazy — iterate to actually receive packets.
