# Low-level pipeline

**Use when:** the high-level [`transcode()`](transcoding.md) and
[`extract_frames()`](frame-extraction.md) APIs don't give you the control you need. This is
**Tier 3**: you wire the pieces together by hand — `MediaReader` → `Decoder` →
`VideoEncoder` → `MediaWriter`.

This guide has two parts: a **read-only** decode loop, then the **full** read → decode →
encode → mux pipeline.

## Part 1 — decode frames (read only)

The minimal loop: open a file, decode its video stream, inspect each frame.

```rust
use media::prelude::*;

let mut reader = MediaReader::open("input.mp4")?;
let vidx = reader.best_stream(StreamKind::Video)?;   // (1)!
let mut decoder = reader.stream(vidx).decoder()?;    // (2)!

println!("{}x{} {:?}", decoder.width(), decoder.height(), decoder.pixel_format());

for packet in reader.packets() {                     // (3)!
    let packet = packet?;
    if packet.stream_index() != vidx { continue; }   // (4)!

    for frame in decoder.decode(&packet)? {          // (5)!
        let frame = frame?;
        // …inspect frame.width(), frame.pixel_format(), frame.best_effort_timestamp()…
        let _ = frame;
    }
}

for frame in decoder.flush()? {                      // (6)!
    let _frame = frame?;
}
# Ok::<(), media::Error>(())
```

1. `best_stream(StreamKind::Video)` returns the index of the best video stream, or
   `Error::NoVideoStream` if there is none.
2. `stream(idx).decoder()` builds a [`Decoder`](../reference/codec.md#decoder) configured from
   that stream. The decoder owns its state and does **not** borrow the reader, so you can
   decode while iterating packets.
3. `packets()` yields every packet in the file (all streams, interleaved).
4. Skip packets that aren't from our video stream.
5. `decode(&packet)` returns an iterator of the frames that packet produced. One packet can
   yield zero, one, or several frames — always drain the iterator fully before the next
   `decode` call (that's FFmpeg's send/receive contract).
6. At end of input, `flush()` drains any frames the decoder is still holding. Skipping this
   drops the tail of the video.

## Part 2 — the full re-encode pipeline

Now feed the decoded frames into an encoder and mux the result. This is the canonical
`VideoEncoder` demo.

### Set up the reader, decoder and encoder

```rust
use media::prelude::*;

let mut reader = MediaReader::open("input.mp4")?;
let vidx = reader.best_stream(StreamKind::Video)?;
let in_tb = reader.stream_time_base(vidx)?;          // (1)!
let fr = reader.stream_avg_frame_rate(vidx)?;

let mut decoder = reader.stream(vidx).decoder()?;
let mut encoder = VideoEncoder::builder()
    .codec(VideoCodec::H264)
    .from_decoder(&decoder)                          // (2)!
    .framerate(Framerate(fr))                        // (3)!
    .time_base(in_tb)                                // (4)!
    .preset(H264Preset::Ultrafast)
    .build()?;
# let _ = (&mut decoder, &mut encoder);
# Ok::<(), media::Error>(())
```

1. Grab the input's **time base** and average **frame rate** up front — the encoder needs both
   so its output timestamps line up with the source.
2. `from_decoder(&decoder)` inherits the resolution, pixel format and frame rate from the
   decoder. The easiest way to keep the input's geometry.
3. Set the output frame rate from the input's. `Framerate` wraps a `Rational`, so
   `Framerate(fr)` reuses the exact rational rate.
4. `time_base(in_tb)` tells the encoder which base the incoming frame timestamps are in.

### Wire the writer and the encode loop

```rust
use media::prelude::*;

// A helper that drains an encoder's packets into the writer, tagging the output stream.
fn drain(
    encoder: &mut VideoEncoder,
    writer: &mut MediaWriter,
    out_idx: usize,
    frame: Option<&Frame>,   // (1)!
) -> media::Result<()> {
    let iter = match frame {
        Some(f) => encoder.encode(f)?,
        None => encoder.flush()?,   // (2)!
    };
    for pkt in iter {
        let mut pkt = pkt?;
        pkt.set_stream_index(out_idx); // (3)!
        writer.write_packet(&mut pkt)?;
    }
    Ok(())
}

# fn run(mut reader: media::MediaReader, mut decoder: media::Decoder, mut encoder: media::VideoEncoder, vidx: usize) -> media::Result<()> {
let mut writer = MediaWriter::create("output.mp4")?;
let out_idx = writer.add_stream_from_encoder(&encoder)?; // (4)!
writer.write_header()?;

for packet in reader.packets() {
    let packet = packet?;
    if packet.stream_index() != vidx { continue; }
    for frame in decoder.decode(&packet)? {
        let mut frame = frame?;
        if let Some(ts) = frame.best_effort_timestamp() {
            frame.set_pts(ts);       // (5)!
        }
        drain(&mut encoder, &mut writer, out_idx, Some(&frame))?;
    }
}

// End of stream: flush the decoder, encode its tail, then flush the encoder.
let tail: Vec<_> = decoder.flush()?.collect::<media::Result<_>>()?; // (6)!
for mut frame in tail {
    if let Some(ts) = frame.best_effort_timestamp() { frame.set_pts(ts); }
    drain(&mut encoder, &mut writer, out_idx, Some(&frame))?;
}
drain(&mut encoder, &mut writer, out_idx, None)?; // (7)!
writer.write_trailer()?;
# Ok(()) }
```

1. The helper handles both the normal case (`Some(frame)` → encode) and the final flush
   (`None`).
2. `encoder.flush()` emits any packets buffered inside the encoder — the encoder may be
   holding a whole group-of-pictures. **Failing to flush truncates the output.**
3. Packets from the encoder carry the encoder's stream index (0); retag them with the output
   stream index before muxing.
4. `add_stream_from_encoder(&encoder)` creates an output video stream matching the encoder and
   returns its index. `write_header()` must follow, before any packet.
5. Re-stamp each frame's PTS from its best-effort timestamp before encoding, so output
   timestamps track the source.
6. Drain the **decoder** first at end of stream (it may hold trailing frames), encoding each.
7. Then drain the **encoder** with a `None` flush. Two separate flushes: decoder, then
   encoder.

!!! danger "Two flushes, in order"
    End-of-stream needs both a **decoder** flush (frames it still holds) *and* an **encoder**
    flush (packets it still holds). Miss either and the last fraction of a second is silently
    dropped.

## Part 3 — building blocks for your own pipeline

A pipeline of your own often needs more than decode and encode. These pieces cover the usual
cases. The [`segments` example](https://github.com/vegidio/media-rs/blob/main/examples/segments.rs)
puts them together: it encodes a video as independent 2-second H.264 segments, keeps them as
bytes, and writes them out as fragmented MP4.

### Shape the frames before encoding

Scale, turn or convert frames with a [`VideoFilter`](../reference/filter.md#videofilter) built for
your decoder (see [Filters](filters.md#running-a-chain-yourself)), and size the encoder from its
output. Two facts about the source decide the shape the picture should be shown at:

```rust
use media::prelude::*;
# fn demo(reader: &mut MediaReader, index: usize) -> media::Result<()> {
let rotation = reader.stream_rotation(index)?;           // (1)!
let decoder = reader.stream(index).decoder()?;
let sar = decoder.sample_aspect_ratio();                  // (2)!

let mut width = f64::from(decoder.width()) * sar.as_f64();
let mut height = f64::from(decoder.height());
if matches!(rotation, Some(90 | 270)) {
    std::mem::swap(&mut width, &mut height);              // (3)!
}
# let _ = (width, height); Ok(()) }
```

1. [`stream_rotation`](../reference/format.md#rotation) says how far the picture must be turned to
   be upright: a phone's portrait video is stored as landscape pixels and turned when shown.
   Decoding drops the turn, so put `transpose=clock` (90), `hflip,vflip` (180) or
   `transpose=cclock` (270) at the start of the chain.
2. `sample_aspect_ratio` is `1:1` for square pixels. For an anamorphic source it is the width of a
   pixel relative to its height, so the shape to show the picture at is the width times it.
3. A quarter turn swaps the sides.

### Tune the encoder

[`option`](../reference/codec.md#videoencoderbuilder) passes any encoder option by FFmpeg's
name, for what the typed setters don't cover:

```rust
use media::prelude::*;
# fn demo(decoder: &Decoder, time_base: Rational) -> media::Result<()> {
let encoder = VideoEncoder::builder()
    .codec(VideoCodec::H264)
    .from_decoder(decoder)
    .time_base(time_base)
    .gop_size(1000)                    // (1)!
    .option("bf", "0")                 // (2)!
    .option("sc_threshold", "0")       // (3)!
    .build()?;
# let _ = encoder; Ok(()) }
```

1. A GOP longer than the frames you will feed it…
2. …with no B-frames, so every packet's `dts` equals its `pts`…
3. …and no scene-cut keyframes gives exactly one keyframe: the first frame. The source's own
   keyframes don't count: `encode` places keyframes by the encoder's settings alone.

An option the encoder doesn't know fails `build` with `Error::UnknownOption`.

### Keep encoded packets

A packet's [`data`, timestamps, duration and keyframe flag](../reference/frame-packet.md#keeping-packets)
are all a muxer needs, and `Packet::from_data` rebuilds it from them. Store them, in memory or on
disk, and write them again later without re-encoding.

## Why go low-level?

You get access things the high-level API doesn't expose: custom frame selection, per-frame
processing between decode and encode, non-standard timestamp handling, or feeding frames from
a source that isn't a file. For everything else, prefer [`transcode()`](transcoding.md) — it
handles the flush choreography (and audio, filters, trimming) for you.

## See also

- [Codec API reference](../reference/codec.md) — `Decoder`, `VideoEncoder`
- [Format I/O API reference](../reference/format.md) — `MediaReader`, `MediaWriter`
- [Frame & Packet reference](../reference/frame-packet.md)
