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

Creates a media file and muxes packets into it.

| Method | Signature | Description |
|--------|-----------|-------------|
| `create` | `create(path: impl AsRef<str>) -> Result<Self>` | Create; container inferred from the extension. |
| `add_stream_from_encoder` | `add_stream_from_encoder(&mut self, encoder: &VideoEncoder) -> Result<usize>` | Add an output stream fed by `encoder`; returns its index. |
| `add_stream_copy` | `add_stream_copy(&mut self, reader: &MediaReader, src_index: usize) -> Result<usize>` | Add a stream that copies `src_index` verbatim (remux); returns its index. |
| `wants_global_header` | `wants_global_header(&self) -> bool` | Whether the container wants codec extradata in its header. |
| `write_header` | `write_header(&mut self) -> Result<()>` | Write the header; once, after all streams, before any packet. |
| `write_packet` | `write_packet(&mut self, packet: &mut Packet) -> Result<()>` | Mux one packet; its `stream_index` selects the output stream and timestamps are rescaled automatically. |
| `write_trailer` | `write_trailer(&mut self) -> Result<()>` | Finalise and close the file. |

### Usage order

1. `create`
2. `add_stream_from_encoder` / `add_stream_copy` (one per output stream)
3. `write_header`
4. `write_packet` per packet (with the correct `stream_index`)
5. `write_trailer`

Calling `write_packet` before `write_header` returns
[`Error::InvalidConfig`](errors.md).
