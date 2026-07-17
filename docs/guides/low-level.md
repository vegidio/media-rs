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

## Why go low-level?

You get access things the high-level API doesn't expose: custom frame selection, per-frame
processing between decode and encode, non-standard timestamp handling, or feeding frames from
a source that isn't a file. For everything else, prefer [`transcode()`](transcoding.md) — it
handles the flush choreography (and audio, filters, trimming) for you.

## See also

- [Codec API reference](../reference/codec.md) — `Decoder`, `VideoEncoder`
- [Format I/O API reference](../reference/format.md) — `MediaReader`, `MediaWriter`
- [Frame & Packet reference](../reference/frame-packet.md)
