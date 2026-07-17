# Audio

Audio encoding, filtering, and resampling. See the [Audio guide](../guides/audio.md). Types
come from `media::transcode` (`AudioConfig`), `media::codec` (`AudioEncoder`, `Resampler`),
`media::filter` (`AudioFilterChain`, `Decibels`), and `media::frame` (`SampleBuffer`).

## `AudioConfig`

How to encode the output audio stream, for [`Transcoder`](transcode.md)'s `.audio(...)`.
Anything unset is inherited from the input (sample rate, channels) or the container (codec).

Build with `AudioConfig::builder()`:

| Method | Signature | Description |
|--------|-----------|-------------|
| `codec` | `codec(AudioCodec) -> Self` | The codec (**required**). |
| `bitrate` | `bitrate(Bitrate) -> Self` | Target bit rate (ignored by lossless codecs). |
| `sample_rate` | `sample_rate(SampleRate) -> Self` | Output sample rate. |
| `channels` | `channels(Channels) -> Self` | Output channel configuration. |
| `sample_format` | `sample_format(SampleFormat) -> Self` | Sample format (defaults to the encoder's preferred). |
| `build` | `build(self) -> Result<AudioConfig>` | Validate; errors `InvalidConfig` without a codec. |

## `AudioFilterChain`

A chain of audio filters, the counterpart to [`VideoFilterChain`](filter.md#videofilterchain).
Empty by default; each operator appends a stage. Setting a chain forces the audio to be
re-encoded.

| Method | Signature | Description |
|--------|-----------|-------------|
| `new` | `new() -> Self` | An empty chain (a no-op). |
| `raw` | `raw(impl Into<String>) -> Self` | A chain from a raw libavfilter string, used verbatim. |
| `volume` / `gain` | `volume(Decibels) -> Self` | Adjust the level by decibels. |
| `resample` | `resample(SampleRate) -> Self` | Resample to a new sample rate. |
| `highpass` | `highpass(hz: f64) -> Self` | Remove content below `hz`. |
| `lowpass` | `lowpass(hz: f64) -> Self` | Remove content above `hz`. |
| `fade_in` | `fade_in(Duration) -> Self` | Fade in from silence over `duration`. |
| `fade_out` | `fade_out(start: Duration, duration: Duration) -> Self` | Fade out over `duration`, starting at `start`. |
| `atempo` | `atempo(factor: f64) -> Self` | Change tempo without changing pitch (`0.5`–`2.0`). |
| `is_empty` | `is_empty(&self) -> bool` | `true` if no stages were added. |
| `description` | `description(&self) -> String` | The combined libavfilter string. |

## `Decibels`

`pub struct Decibels(pub f64)` — a gain amount in decibels, used by `AudioFilterChain::gain`.

## `AudioEncoder`

A configured, opened audio encoder (module `media::codec`). It resamples every input
[`Frame`](frame-packet.md) to the encode format and buffers samples to the codec's required
frame size, so `encode` returns a `Vec<Packet>` rather than a lazy iterator.

Build with `AudioEncoder::builder()`:

| Method | Signature | Description |
|--------|-----------|-------------|
| `codec` | `codec(AudioCodec) -> Self` | The codec (**required**). |
| `bitrate` | `bitrate(Bitrate) -> Self` | Target bit rate. |
| `sample_rate` | `sample_rate(SampleRate) -> Self` | Output sample rate (defaults via `from_decoder`, else 44.1 kHz). |
| `channels` | `channels(Channels) -> Self` | Output channels (defaults to the decoder's, else stereo). |
| `sample_format` | `sample_format(SampleFormat) -> Self` | Encode format (defaults to the encoder's preferred). |
| `from_decoder` | `from_decoder(&Decoder) -> Self` | Inherit sample rate + channel layout. |
| `global_header` | `global_header(bool) -> Self` | Global-header flag (on by default; for MP4/MKV). |
| `build` | `build(self) -> Result<AudioEncoder>` | Validate and open. |

Then:

| Method | Signature | Description |
|--------|-----------|-------------|
| `encode` | `encode(&Frame) -> Result<Vec<Packet>>` | Resample + buffer + encode; returns any ready packets. |
| `flush` | `flush(&mut self) -> Result<Vec<Packet>>` | Drain the resampler tail + FIFO at end of stream. |
| `time_base` | `time_base(&self) -> Rational` | The encoder time base (`1/sample_rate`). |

Register it on a writer with [`MediaWriter::add_stream_from_encoder`](format.md), which accepts
a `VideoEncoder` or an `AudioEncoder`.

## `Resampler`

Standalone audio resampling between sample formats, rates, and channel layouts (module
`media::codec`). Build with `Resampler::builder()`:

| Method | Signature | Description |
|--------|-----------|-------------|
| `input_format` | `input_format(SampleFormat, SampleRate, Channels) -> Self` | The input format (**required**). |
| `output_format` | `output_format(SampleFormat, SampleRate, Channels) -> Self` | The output format (**required**). |
| `build` | `build(self) -> Result<Resampler>` | Validate and build. |
| `convert` | `convert(&Frame) -> Result<Frame>` | Convert a frame to the output format. |
| `flush` | `flush(&mut self) -> Result<Frame>` | Drain buffered samples at end of stream. |

## `SampleBuffer`

Mutable, typed access to a decoded audio frame's PCM samples, from
[`Frame::samples_mut`](frame-packet.md). Variants mirror
[`SampleFormat`](types.md#sampleformat): interleaved formats hold one slice packed across
channels; planar formats (`…p`) hold one slice per channel.

```rust
pub enum SampleBuffer<'a> {
    U8(&'a mut [u8]),   S16(&'a mut [i16]),   S32(&'a mut [i32]),
    Flt(&'a mut [f32]), Dbl(&'a mut [f64]),
    S16p(Vec<&'a mut [i16]>), S32p(Vec<&'a mut [i32]>),
    Fltp(Vec<&'a mut [f32]>), Dblp(Vec<&'a mut [f64]>),
    Other, // non-audio frame or a format not exposed as typed slices
}
```
