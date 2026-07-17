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
| `rescale_ts` | `rescale_ts(&mut self, src: Rational, dst: Rational)` | Rescale timestamps between time bases. |
| `clear_pos` | `clear_pos(&mut self)` | Reset the byte position so the muxer recomputes it. |
| `offset_timestamps` | `offset_timestamps(&mut self, delta: i64)` | Shift pts/dts earlier by `delta` (re-basing trimmed streams). |

!!! note "Muxing handles rescaling for you"
    When you call [`MediaWriter::write_packet`](format.md#mediawriter), it rescales the
    packet's timestamps from the source time base into the output stream's base automatically.
    You typically only need `set_stream_index` before writing a remuxed packet.
