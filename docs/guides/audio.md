# Audio

**Use when:** you want to convert audio formats, change the codec/bit rate/sample rate/channel
layout, apply audio filters, or process raw PCM samples. Audio follows the same
[three-tier API](../getting-started/concepts.md#the-three-tier-api) as video: a one-liner, a
builder, and a hand-wired frame-level pipeline.

!!! note "Copied by default, re-encoded on demand"
    A plain `transcode(...)` **stream-copies** audio (fast, lossless). Audio is re-encoded only
    when you ask — via `.audio(...)` or an audio filter — or when the source codec can't fit the
    target container (e.g. PCM WAV → `.mp3`), in which case the container's default codec is used
    automatically.

## Tier 1 — the one-liner

Format conversion is just a different output extension:

```rust
use media::prelude::*;
# fn demo() -> media::Result<()> {
transcode("song.wav").to("song.mp3").run()?;          // (1)!

transcode("podcast.wav")
    .to("podcast.m4a")
    .audio(
        AudioConfig::builder()
            .codec(AudioCodec::Aac)                     // (2)!
            .bitrate(Bitrate::kbps(128))                // (3)!
            .sample_rate(SampleRate::Hz44100)           // (4)!
            .channels(Channels::Stereo)                 // (5)!
            .build()?,
    )
    .run()?;
# Ok(()) }
```

1. The `.mp3` container can't hold PCM, so the audio is auto-encoded to MP3 — no codec to set.
2. `.codec(...)` picks the encoder explicitly (required by the builder).
3. `.bitrate(...)` takes a [`Bitrate`](../reference/types.md#bitrate) (`kbps`/`mbps`).
4. `.sample_rate(...)` resamples to a new rate.
5. `.channels(...)` down- or up-mixes (e.g. `Channels::Mono`).

To extract audio from a video file, `transcode_audio(...)` is a shorthand for
`transcode(...).drop_video()`:

```rust
use media::prelude::*;
# fn demo() -> media::Result<()> {
transcode_audio("movie.mp4").to("song.mp3").run()?;    // (1)!
# Ok(()) }
```

1. Targeting an audio-only container with a video input **without** dropping video is an error
   (`transcode("movie.mp4").to("song.mp3")` fails loudly) — `transcode_audio` makes the intent
   explicit.

## Tier 2 — the builder

Build an [`AudioConfig`](../reference/audio.md#audioconfig) and hand it to `.audio(...)` — on
either the one-liner or the `Transcoder` builder (it's the same method):

```rust
use media::prelude::*;
# fn demo() -> media::Result<()> {
let audio = AudioConfig::builder()
    .codec(AudioCodec::Aac)                             // (1)!
    .bitrate(Bitrate::kbps(192))
    .sample_rate(SampleRate::Hz48000)
    .channels(Channels::Stereo)
    .build()?;                                          // (2)!

Transcoder::builder()
    .input("input.mov")
    .output("output.mp4")
    .video(VideoConfig::builder().codec(VideoCodec::H264).build()?)
    .audio(audio)                                       // (3)!
    .build()?
    .run()?;
# Ok(()) }
```

1. `codec` is the only required field; everything else is inherited from the input if unset.
2. `build()` validates the config, returning `Error::InvalidConfig` if the codec is missing.
3. `.video(...)` and `.audio(...)` populate two independent streams of the **same** job — the
   writer interleaves the packets by timestamp for you, so A/V stays in sync automatically.

### Audio filters

[`AudioFilterChain`](../reference/audio.md#audiofilterchain) mirrors `VideoFilterChain` for
audio-specific operations. Setting a filter forces the audio to be re-encoded:

```rust
use media::prelude::*;
# fn demo() -> media::Result<()> {
let filters = AudioFilterChain::new()
    .highpass(80.0)                                     // (1)!
    .lowpass(15_000.0)
    .gain(Decibels(3.0));                               // (2)!

transcode("raw_recording.wav")
    .to("clean.m4a")
    .audio_filter(filters)                              // (3)!
    .run()?;
# Ok(()) }
```

1. `highpass`/`lowpass` cut rumble and harsh highs (frequencies in Hz).
2. `gain` adjusts the level in [`Decibels`](../reference/audio.md#decibels). Other operators:
   `resample`, `fade_in`, `fade_out`, `atempo`, `volume`.
3. For anything not covered, `AudioFilterChain::raw("loudnorm=I=-16:TP=-1.5:LRA=11")` passes a
   libavfilter string through verbatim.

## Tier 3 — the sample level

Drop to the frame-level API for custom DSP over raw PCM. Build an
[`AudioEncoder`](../reference/audio.md#audioencoder) with `from_decoder`, and touch samples via
[`Frame::samples_mut`](../reference/frame-packet.md) and
[`SampleBuffer`](../reference/audio.md#samplebuffer):

```rust
use media::prelude::*;
# fn demo() -> media::Result<()> {
let mut reader = MediaReader::open("input.wav")?;
let aidx = reader.best_stream(StreamKind::Audio)?;
let mut decoder = reader.stream(aidx).decoder()?;

let mut encoder = AudioEncoder::builder()
    .codec(AudioCodec::Flac)
    .from_decoder(&decoder)                             // (1)!
    .build()?;

let mut writer = MediaWriter::create("output.flac")?;
let out = writer.add_stream_from_encoder(&encoder)?;    // (2)!
writer.write_header()?;

for packet in reader.packets() {
    let packet = packet?;
    if packet.stream_index() != aidx { continue; }
    for frame in decoder.decode(&packet)? {
        let mut frame = frame?;
        if let SampleBuffer::Fltp(planes) = frame.samples_mut() {  // (3)!
            for plane in planes {
                for s in plane.iter_mut() { *s *= 0.5; }           // halve the gain
            }
        }
        for mut pkt in encoder.encode(&frame)? {                   // (4)!
            pkt.set_stream_index(out);
            writer.write_packet(&mut pkt)?;
        }
    }
}
for mut pkt in encoder.flush()? {                                  // (5)!
    pkt.set_stream_index(out);
    writer.write_packet(&mut pkt)?;
}
writer.write_trailer()?;
# Ok(()) }
```

1. `from_decoder` inherits the input's sample rate and channel layout.
2. `add_stream_from_encoder` accepts a `VideoEncoder` **or** an `AudioEncoder` — one method for
   both stream kinds.
3. `samples_mut()` returns a [`SampleBuffer`](../reference/audio.md#samplebuffer) matched to the
   frame's sample format. The `AudioEncoder` resamples to the encode format internally, so you
   can process the decoder's native format directly.
4. `encode(&frame)` returns a `Vec<Packet>` (an input frame may map to zero or several encoded
   frames, since the encoder buffers to its fixed frame size).
5. `flush()` drains the encoder's internal FIFO — skip it and the tail of the audio is lost.

### Resampling

For standalone format conversion (e.g. feeding a fixed-format encoder or a real-time sink), use
a [`Resampler`](../reference/audio.md#resampler):

```rust
use media::prelude::*;
# fn demo(frame: &Frame) -> media::Result<()> {
let mut resampler = Resampler::builder()
    .input_format(SampleFormat::S16, SampleRate::Hz44100, Channels::Stereo)
    .output_format(SampleFormat::Fltp, SampleRate::Hz48000, Channels::Mono)
    .build()?;

let converted = resampler.convert(frame)?;             // (1)!
# let _ = converted; Ok(()) }
```

1. `convert` returns a new `Frame` in the output format; at end of stream call `flush()` to
   drain any samples the converter buffered.

## See also

- [Audio API reference](../reference/audio.md)
- [Transcoding](transcoding.md) — the shared transcode pipeline
- [Filters](filters.md) — the video counterpart to `AudioFilterChain`
