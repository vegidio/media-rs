# Codec

Module `media::codec`. Frame-level decoding and encoding. See the
[Low-level pipeline guide](../guides/low-level.md).

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
| `encode` | `encode(&mut self, frame: &Frame) -> Result<EncodeIter<'_>>` | Submit a frame (PTS must be in the encoder's time base). |
| `flush` | `flush(&mut self) -> Result<EncodeIter<'_>>` | Drain buffered packets at end of stream. |
| `time_base` | `time_base(&self) -> Rational` | The encoder's time base. |

!!! danger "Flush or truncate"
    At end of stream you **must** drain `flush()` — the encoder may hold a whole GOP. Skipping
    it silently truncates the output.

### `VideoEncoderBuilder`

| Method | Signature | Description |
|--------|-----------|-------------|
| `codec` | `codec(codec: VideoCodec) -> Self` | Codec (**required**). |
| `resolution` | `resolution(width: u32, height: u32) -> Self` | Output size (**required** unless `from_decoder`). |
| `from_decoder` | `from_decoder(decoder: &Decoder) -> Self` | Inherit resolution, pixel format and frame rate. |
| `pixel_format` | `pixel_format(pix_fmt: PixelFormat) -> Self` | Pixel format (default: decoder's, else YUV420p). |
| `framerate` | `framerate(framerate: Framerate) -> Self` | Output frame rate (default 25). |
| `time_base` | `time_base(time_base: Rational) -> Self` | Time base for incoming frame timestamps. |
| `bitrate` | `bitrate(bitrate: Bitrate) -> Self` | Target bit rate. |
| `preset` | `preset(preset: H264Preset) -> Self` | Speed/quality preset (H.264/H.265). |
| `profile` | `profile(profile: H264Profile) -> Self` | Codec profile (H.264/H.265). |
| `gop_size` | `gop_size(gop_size: u32) -> Self` | Keyframe interval (default 12). |
| `global_header` | `global_header(enabled: bool) -> Self` | Global-header flag (**on by default**; needed for MP4/MKV/WebM). |
| `build` | `build(self) -> Result<VideoEncoder>` | Validate and open the encoder. |

`build` requires a codec and a resolution; a zero/out-of-range resolution →
[`Error::UnsupportedResolution`](errors.md), a missing codec/resolution →
[`Error::InvalidConfig`](errors.md).

### `EncodeIter`

`Iterator<Item = Result<Packet>>` over the packets from one `encode`/`flush` call. Encoding is
lazy — iterate to actually receive packets.
