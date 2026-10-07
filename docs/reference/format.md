# Format I/O

Module `media::format`. Reading/demuxing and writing/muxing. See the
[Remuxing](../guides/remuxing.md) and [Low-level pipeline](../guides/low-level.md) guides.

## `MediaReader`

Opens a media file and exposes its streams and packets.

| Method | Signature | Description |
|--------|-----------|-------------|
| `open` | `open(path: impl AsRef<str>) -> Result<Self>` | Open and probe the stream layout. |
| `stream_count` | `stream_count(&self) -> usize` | Number of streams. |
| `duration_secs` | `duration_secs(&self) -> f64` | Estimated duration (`0.0` if unknown). |
| `best_stream` | `best_stream(&self, kind: StreamKind) -> Result<usize>` | Index of the best stream of `kind`; errors [`NoVideoStream`](errors.md)/[`NoAudioStream`](errors.md). |
| `stream` | `stream(&mut self, index: usize) -> StreamRef<'_>` | A handle to one stream. |
| `stream_kind` | `stream_kind(&self, index: usize) -> Result<StreamKind>` | The stream's kind. |
| `stream_time_base` | `stream_time_base(&self, index: usize) -> Result<Rational>` | The stream's time base. |
| `stream_avg_frame_rate` | `stream_avg_frame_rate(&self, index: usize) -> Result<Rational>` | Average frame rate (may be `0/0`). |
| `seek` | `seek(&mut self, stream_index: usize, at: Duration) -> Result<()>` | Seek to at/before the nearest keyframe; call [`Decoder::reset`](codec.md#decoder) after. |
| `packets` | `packets(&mut self) -> Packets<'_>` | Iterate every packet in interleaved order. |

!!! note "`stream` borrows mutably"
    `stream(&mut self, …)` takes `&mut self` because a [`StreamRef`](#streamref) may need to
    seek (for `sampled_at`). Build a `Decoder` from the handle up front; the decoder itself
    does not borrow the reader, so you can then decode while iterating `packets()`.

### `StreamRef`

A handle to one stream, borrowing the reader for its lifetime.

| Method | Signature | Description |
|--------|-----------|-------------|
| `index` | `index(&self) -> usize` | The stream index. |
| `kind` | `kind(&self) -> Result<StreamKind>` | The stream's media kind. |
| `decoder` | `decoder(&self) -> Result<Decoder>` | Build a [`Decoder`](codec.md#decoder) for this stream. |
| `sampled_at` | `sampled_at(self, interval: Interval) -> Result<SampledFrames>` | Tier-3 frame sampling iterator (video). |

### `Packets`

Iterator of `Result<Packet>` over the reader's packets, in interleaved order.

## `MediaWriter`

Muxes packets into a file, or into any `std::io::Write`.

| Method | Signature | Description |
|--------|-----------|-------------|
| `create` | `create(path: impl AsRef<str>) -> Result<Self>` | Create; container inferred from the extension. Same as `builder().path(path).build()`. |
| `builder` | `builder() -> MediaWriterBuilder` | Write to a path or a `Write`, with an explicit container and muxer options. See [`MediaWriterBuilder`](#mediawriterbuilder). |
| `add_stream_from_encoder` | `add_stream_from_encoder(&mut self, encoder: &VideoEncoder) -> Result<usize>` | Add an output stream fed by `encoder`; returns its index. |
| `add_stream_copy` | `add_stream_copy(&mut self, reader: &MediaReader, src_index: usize) -> Result<usize>` | Add a stream that copies `src_index` verbatim (remux); returns its index. |
| `wants_global_header` | `wants_global_header(&self) -> bool` | Whether the container wants codec extradata in its header. |
| `write_header` | `write_header(&mut self) -> Result<()>` | Write the header; once, after all streams, before any packet. |
| `write_packet` | `write_packet(&mut self, packet: &mut Packet) -> Result<()>` | Mux one packet; its `stream_index` selects the output stream and timestamps are rescaled automatically. |
| `flush` | `flush(&mut self) -> Result<()>` | Write out everything muxed so far. For fragmented MP4 it also closes the pending fragment. Only after `write_header`. |
| `write_trailer` | `write_trailer(&mut self) -> Result<()>` | Finalise and close the file. |

### Usage order

1. `create`
2. `add_stream_from_encoder` / `add_stream_copy` (one per output stream)
3. `write_header`
4. `write_packet` per packet (with the correct `stream_index`)
5. `write_trailer`

Calling `write_packet` or `flush` before `write_header` returns
[`Error::InvalidConfig`](errors.md).

### `flush`

`flush` does three things, in order:

1. Writes every packet still waiting in the interleaving queue. `write_packet` interleaves streams by timestamp, so
   it can hold packets back.
2. Asks the muxer to close what it has pending. The mp4 muxer cuts its current fragment; a muxer that can't cut on
   demand ignores the request.
3. Hands the I/O buffer (64 KiB) to the file or writer.

So with [`fragmented_mp4`](#mediawriterbuilder), after each `flush` the output holds the init segment and only whole
`moof` + `mdat` fragments. Call it before writing each video keyframe ([`Packet::is_keyframe`](frame-packet.md#packet))
to hand a player one fragment at a time.

## `MediaWriterBuilder`

Built by `MediaWriter::builder()`; by-value setters, then `build()`.

```rust
use media::prelude::*;
# fn demo(sink: std::fs::File) -> media::Result<()> {
let writer = MediaWriter::builder()
    .writer(sink)                       // or .path("out.mkv")
    .fragmented_mp4()                   // the mp4 container + the movflags MSE needs
    .option("frag_duration", "2000000") // any muxer option, in microseconds here
    .build()?;
# Ok(()) }
```

| Method | Signature | Description |
|--------|-----------|-------------|
| `path` | `path(self, path: impl AsRef<str>) -> Self` | Write to a file. |
| `writer` | `writer(self, w: impl Write + Send + 'static) -> Self` | Write to `w` instead. Needs `format`. Never seeked. |
| `format` | `format(self, name: impl AsRef<str>) -> Self` | FFmpeg's muxer name: `mp4`, `matroska`, `webm`, `mpegts`, … Required with `writer`; overrides the extension with `path`. |
| `option` | `option(self, key, value) -> Self` | A muxer option (`movflags`, `frag_duration`, …). Repeatable; a later value for the same key wins. |
| `fragmented_mp4` | `fragmented_mp4(self) -> Self` | `format("mp4")` plus `movflags=frag_keyframe+empty_moov+default_base_moof+skip_trailer`, added to any `movflags` set with `option`, before or after it. Other options are kept. |
| `build` | `build(self) -> Result<MediaWriter>` | Open the file or wrap the writer. |

`build` returns [`Error::InvalidConfig`](errors.md) when given neither or both of `path` and `writer`, or a `writer`
without a `format`. Options are checked at `write_header`: one the muxer doesn't recognise is
[`Error::UnknownOption`](errors.md), raised before anything is written, so a typo can't quietly produce an
unfragmented file.

### Fragmented MP4

`fragmented_mp4()` produces what Media Source Extensions expect:

- an init segment, `ftyp` + `moov`, with no samples in it;
- then `moof` + `mdat` fragments, each starting on a video keyframe, with offsets relative to its own `moof`;
- and no `mfra` index at the end.

`frag_keyframe` makes fragments as long as the source's keyframe interval, so a 10 s GOP gives 10 s fragments. Use
`option("frag_duration", …)` (microseconds) or `flush()` to cut shorter ones.

### Writers must not seek, and are `'static`

The output is written front to back, so only containers that don't need to go back and patch a header work with a
`writer`: fragmented MP4, Matroska/WebM, MPEG-TS, … Plain `mp4` fails at `write_header`, before writing anything,
because it must seek back to write its `moov`.

The writer is moved into the `MediaWriter`, hence `'static`. To get the bytes back, give it something you share:

```rust
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct SharedBuf(Arc<Mutex<Vec<u8>>>);

impl Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

// let buf = SharedBuf::default();
// MediaWriter::builder().writer(buf.clone()).fragmented_mp4().build()?;
// … buf.0.lock().unwrap() holds everything written so far.
```

A channel-backed writer (each `write` sends a `Vec<u8>`) works the same way and hands bytes to another thread.

!!! warning "The writer runs inside FFmpeg"
    `write` is called from FFmpeg's muxing code, on the thread calling `write_packet`, `flush` or
    `write_trailer`. An error it returns, or a panic, is caught and comes back from that call as
    [`Error::Write`](errors.md) carrying the writer's error. A writer that blocks blocks the muxing thread, so from
    async code, write to a bounded channel from a worker thread rather than awaiting inside `write`.
