# Probe

Module `media::probe`. Quick inspection of a media file without decoding. See the
[Probing guide](../guides/probing.md).

## `probe`

```rust
pub fn probe(path: impl AsRef<str>) -> Result<MediaInfo>
```

Open `path` and return its container/stream metadata. Does not decode any frames.

## `MediaInfo`

Container-level metadata.

| Method | Returns | Description |
|--------|---------|-------------|
| `duration` | `Duration` | The container's estimated duration. |
| `stream_count` | `usize` | The number of streams. |
| `streams` | `&[StreamInfo]` | All streams, in container order. |
| `video` | `Option<&StreamInfo>` | The first video stream, if any. |
| `audio` | `Option<&StreamInfo>` | The first audio stream, if any. |

## `StreamInfo`

Per-stream metadata. All fields are public.

```rust
pub struct StreamInfo {
    pub index: usize,                     // position within the container
    pub kind: StreamKind,                 // Video / Audio / Subtitle / Data / Other
    pub width: u32,                       // pixels (video; 0 otherwise)
    pub height: u32,                      // pixels (video; 0 otherwise)
    pub sample_rate: u32,                 // Hz (audio; 0 otherwise)
    pub video_codec: Option<VideoCodec>,  // Some for a recognised video codec
    pub audio_codec: Option<AudioCodec>,  // Some for a recognised audio codec
}
```

The codec fields are `Option`s because only a known set of codecs is enumerated (see
[Types](types.md)); an unrecognised codec still appears with its `kind` and dimensions but a
`None` codec.
