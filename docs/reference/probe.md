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
| `format_name` | `&str` | FFmpeg's demuxer name, e.g. `mov,mp4,m4a,3gp,3g2,mj2` or `matroska,webm`. It can't tell MP4 from MOV, or WebM from Matroska. |
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
    pub codec_name: String,               // FFmpeg's codec name, for any codec: "h264", "mpeg4", …
    pub codec_string: Option<String>,     // RFC 6381 codec string: "avc1.64001f", "mp4a.40.2", …
    pub frame_rate: Option<Framerate>,    // average frame rate (video; None otherwise or if undeclared)
}
```

!!! warning "`StreamInfo` gained fields in 26.10.3"
    `codec_name` and `codec_string` are new public fields. Code that builds a `StreamInfo` with a struct literal,
    or destructures one exhaustively, needs `..` (or the new fields) to compile.

`codec_name` is `avcodec_get_name`'s answer, so it covers every codec, unlike `video_codec`/`audio_codec`.

`codec_string` is FFmpeg's `av_mime_codec_str`, given the stream's average frame rate for video. It is `None` when
FFmpeg has no string for the codec (WMV, ProRes, PCM, …) or the parameters are too incomplete to build one (H.264
without `avcC`, HEVC tagged `hev1`). Some strings are WebM's short names (`vp8`, `opus`), and MPEG-4 Part 2's
`mp4v.20` leaves out its profile and level. See [Probing](../guides/probing.md#container-name-and-codec-strings).

`frame_rate` is the stream's *average* frame rate (FFmpeg's `avg_frame_rate`), which is what players show — not
the timebase-derived base rate, which for a variable-rate phone video often reads 60 or 90000/1.

The codec fields are `Option`s because only a known set of codecs is enumerated (see
[Types](types.md)); an unrecognised codec still appears with its `kind` and dimensions but a
`None` codec.
