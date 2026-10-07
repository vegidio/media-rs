# Remuxing

**Use when:** you want to change a file's **container** without touching the encoded video or
audio — e.g. MP4 → MKV. This is a *stream copy*: no decode, no re-encode. It's dramatically
faster than a transcode and completely lossless.

## The whole thing

```rust
use media::prelude::*;

let mut reader = MediaReader::open("input.mp4")?;   // (1)!
let mut writer = MediaWriter::create("output.mkv")?; // (2)!

// One copied output stream per input stream; remember the index mapping.
let mut out_index = Vec::with_capacity(reader.stream_count());
for i in 0..reader.stream_count() {
    out_index.push(writer.add_stream_copy(&reader, i)?); // (3)!
}

writer.write_header()?;                              // (4)!

for packet in reader.packets() {                     // (5)!
    let mut packet = packet?;
    packet.set_stream_index(out_index[packet.stream_index()]); // (6)!
    writer.write_packet(&mut packet)?;               // (7)!
}

writer.write_trailer()?;                             // (8)!

// Confirm the new container holds the same streams and is decodable.
let info = probe("output.mkv")?;                     // (9)!
println!("{} streams, {:.2}s", info.stream_count(), info.duration().as_secs_f64());
# Ok::<(), media::Error>(())
```

1. Open the source. [`MediaReader`](../reference/format.md#mediareader) demuxes packets but
   doesn't decode them here.
2. Create the destination. The container is inferred from the `.mkv` extension.
3. `add_stream_copy(&reader, i)` adds an output stream that copies input stream `i`'s codec
   parameters verbatim, and returns the **new** stream index. We store the input→output
   mapping in `out_index`.
4. `write_header()` finalises the stream set and writes the container header. It must be
   called once, after all streams are added and before any packets.
5. `reader.packets()` iterates every packet in interleaved order — video and audio both.
6. The packet's `stream_index` still refers to the **input** layout; remap it to the matching
   output stream. That's the only change each packet needs.
7. `write_packet` muxes it. It picks the output stream from the packet's index and rescales
   the timestamps into that stream's time base automatically, so the PTS/DTS the demuxer
   produced carry over correctly.
8. `write_trailer()` finalises and closes the file (writes the index, patches the header).
   Skipping it produces a truncated/unplayable file.
9. Reopening with [`probe`](probing.md) is a cheap sanity check that the output is real.

## Why remux instead of transcode?

| | Remux (`add_stream_copy`) | Transcode |
|---|---|---|
| Re-encodes | No | Yes (video) |
| Quality | Lossless (identical bytes) | Lossy (re-encoded) |
| Speed | Very fast (I/O-bound) | Slow (CPU-bound) |
| Changes codec | No | Yes |
| Changes container | Yes | Yes |

Reach for remuxing whenever the codecs are already what you want and only the wrapper needs to
change.

!!! note "Codecs must fit the target container"
    A stream copy keeps the existing codec, so the destination container has to support it
    (e.g. you can't copy VP9 into a plain `.mp4` the way MKV accepts it). If in doubt, MKV
    accepts almost anything.

## Streaming fragmented MP4

A web view's Media Source Extensions (MSE) player can't open an MKV, but it can play the same H.264/AAC streams
once they're in **fragmented MP4**, fed to it in pieces. Remux into any `std::io::Write` and hand over each fragment
as soon as it's complete:

```rust
use media::prelude::*;
use std::io::Write;

# fn demo(sink: impl Write + Send + 'static) -> media::Result<()> {
let mut reader = MediaReader::open("input.mkv")?;
let mut writer = MediaWriter::builder()             // (1)!
    .writer(sink)                                   // (2)!
    .fragmented_mp4()                               // (3)!
    .build()?;

let mut out_index = Vec::with_capacity(reader.stream_count());
for i in 0..reader.stream_count() {
    out_index.push(writer.add_stream_copy(&reader, i)?);
}
let video = reader.best_stream(StreamKind::Video)?;

writer.write_header()?;                             // (4)!

for packet in reader.packets() {
    let mut packet = packet?;
    if packet.stream_index() == video && packet.is_keyframe() {
        writer.flush()?;                            // (5)!
    }
    packet.set_stream_index(out_index[packet.stream_index()]);
    writer.write_packet(&mut packet)?;
}

writer.write_trailer()?;                            // (6)!
# Ok(()) }
```

1. [`MediaWriter::builder()`](../reference/format.md#mediawriterbuilder) instead of `create`, because the output
   isn't a file.
2. Anything that implements `Write + Send + 'static`: a socket, a pipe, a channel-backed writer, or a shared
   buffer (see [the recipe](../reference/format.md#writers-must-not-seek-and-are-static)). It is never seeked.
3. Shorthand for `.format("mp4")` plus the `movflags` that make the output an init segment (`ftyp` + `moov`)
   followed by `moof` + `mdat` fragments, each starting on a keyframe. Add more with `.option(key, value)`, before or
   after it: your own `movflags` are combined with these. One the muxer doesn't know fails at `write_header` with
   `Error::UnknownOption`.
4. The init segment is written here. Append it to the MSE `SourceBuffer` first.
5. `flush` closes the pending fragment and pushes it into the writer, so the player gets each fragment the moment
   it's complete instead of when a 64 KiB buffer fills. Flushing right before each keyframe gives exactly one
   fragment per flush.
6. Writes the last fragment.

!!! note "The codecs must suit the browser"
    Remuxing only changes the container. A web view still has to decode the streams inside, so check
    [`codec_string`](probing.md#container-name-and-codec-strings) with `MediaSource.isTypeSupported` first. If the
    codecs themselves won't play, transcode instead.

## See also

- [Format I/O API reference](../reference/format.md) — `MediaReader` / `MediaWriter`
- [Low-level pipeline](low-level.md) — the same writer, but fed by an encoder
