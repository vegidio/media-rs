# Frame & Packet

Modules `media::frame` and `media::packet`. The two data types that flow through the
frame-level pipeline.

## `Frame`

A decoded (uncompressed) frame — a single image (video) or a buffer of audio samples. Frames
yielded by a decoder own their data outright, so you can keep, buffer, or process them freely.

| Method | Returns | Description |
|--------|---------|-------------|
| `width` | `u32` | Width in pixels (video; `0` for audio). |
| `height` | `u32` | Height in pixels (video; `0` for audio). |
| `pixel_format` | `PixelFormat` | Pixel format (video frames). |
| `sample_format` | `SampleFormat` | Sample format (audio frames). |
| `sample_count` | `u32` | Samples per channel (audio frames). |
| `sample_rate` | `u32` | Sample rate in Hz (audio frames). |
| `pts` | `Option<i64>` | Presentation timestamp in the source time base (`None` if unknown). |
| `set_pts` | `set_pts(&mut self, pts: i64)` | Set the PTS — in the **target encoder's** time base before encoding. |
| `best_effort_timestamp` | `Option<i64>` | The decoder's best timestamp estimate; prefer this when re-encoding. |

## `Packet`

A compressed, coded chunk of data belonging to one stream — the output of a demuxer or an
encoder, the input to a decoder or a muxer.

| Method | Returns | Description |
|--------|---------|-------------|
| `stream_index` | `usize` | The stream this packet belongs to. |
| `set_stream_index` | `set_stream_index(&mut self, index: usize)` | Set the stream index (remapping input → output). |
| `pts` | `i64` | Presentation timestamp, in the stream's time base. |
| `dts` | `i64` | Decompression timestamp, in the stream's time base. |
| `is_keyframe` | `bool` | Whether a decoder can start from this packet. Flush a fragmented MP4 writer before one to cut a fragment there. |
| `duration` | `i64` | How long the packet lasts, in the stream's time base; `0` when unknown. |
| `data` | `&[u8]` | The compressed payload, exactly as the demuxer read it or the encoder produced it. |
| `from_data` | `from_data(data: &[u8], pts: i64, dts: i64, duration: i64, keyframe: bool) -> Result<Packet>` | A packet from its parts: a copy of `data`, with its timestamps and duration. Its stream index is `0` until you set it. |
| `rescale_ts` | `rescale_ts(&mut self, src: Rational, dst: Rational)` | Rescale timestamps between time bases. |
| `clear_pos` | `clear_pos(&mut self)` | Reset the byte position so the muxer recomputes it. |
| `offset_timestamps` | `offset_timestamps(&mut self, delta: i64)` | Shift pts/dts earlier by `delta` (re-basing trimmed streams). |

### Keeping packets

`data`, `pts`, `dts`, `duration` and `is_keyframe` are everything a muxer needs from a packet,
and `from_data` rebuilds one from them. So encoded packets can be stored, in memory or on disk,
and written again later: a cache of encoded segments, for example.

```rust
use media::prelude::*;
# fn demo(packet: &Packet) -> media::Result<()> {
let kept = (packet.data().to_vec(), packet.pts(), packet.dts(), packet.duration(), packet.is_keyframe());

let (data, pts, dts, duration, keyframe) = kept;
let mut again = Packet::from_data(&data, pts, dts, duration, keyframe)?;
again.set_stream_index(0);
# Ok(()) }
```

Rebuilt packets write the same bytes as the originals, provided they go to a stream with the same
parameters and time base. Packet side data isn't kept.

!!! note "Muxing handles rescaling for you"
    When you call [`MediaWriter::write_packet`](format.md#mediawriter), it rescales the
    packet's timestamps from the source time base into the output stream's base automatically.
    You typically only need `set_stream_index` before writing a remuxed packet.
