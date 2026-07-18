# Errors

Module `media::error`. The error type and result alias for the safe API.

## `Result`

```rust
pub type Result<T> = std::result::Result<T, Error>;
```

Every fallible call in the crate returns this. It's re-exported by the prelude, so `Result<T>`
in your `media`-using code refers to this alias.

## `Error`

Most failures from FFmpeg surface as `Error::Internal` (the last row below), carrying the raw
code and the message FFmpeg's `av_strerror` produced. The other variants are raised by the crate when it
can give you something more actionable than a numeric code.

| Variant | Meaning |
|---------|---------|
| `CodecUnavailable(String)` | The requested codec isn't present in this FFmpeg build. The string is the encoder/decoder name. |
| `NoVideoStream` | The input has no video stream but the operation requires one. |
| `NoAudioStream` | The input has no audio stream but the operation requires one. |
| `StreamOutOfRange(usize)` | The requested stream index doesn't exist. |
| `UnsupportedResolution { width: i32, height: i32 }` | A resolution the chosen codec can't encode (e.g. zero or out of range). |
| `OpenInput(String)` | The input couldn't be opened (missing file, unknown format, permissions). Carries FFmpeg's decoded reason. |
| `CreateOutput(String)` | The output couldn't be created. |
| `AllocFailed(&'static str)` | An allocation inside FFmpeg returned null. |
| `InvalidConfig(&'static str)` | A builder/config was used incorrectly (missing required field, wrong call order). |
| `Bug(&'static str)` | An internal invariant was violated — a bug in this crate, not caller error. Please report it. |
| `ThreadPanicked` | A worker thread panicked during processing (e.g. the transcode demux/decode thread). |
| `InvalidPath` | A path contained an interior NUL byte. |
| `ImageEncode(String)` | Encoding a frame to an image, or writing it out, failed. |
| `Internal { code: i32, message: String }` | A raw FFmpeg error code plus its decoded message. |

### `OpenInput` / `CreateOutput` carry the reason

These strings include FFmpeg's decoded explanation, so a missing file reads like:

```text
could not open input 'x.mp4 (No such file or directory)'
```

### Matching on errors

`Error` derives `Debug` and implements `std::error::Error` (via `thiserror`), so it composes
with `?`, `Box<dyn Error>`, and `anyhow`. Match on specific variants when you want to react:

```rust
use media::prelude::*;

match probe("missing.mp4") {
    Ok(info) => println!("{} streams", info.stream_count()),
    Err(Error::OpenInput(reason)) => eprintln!("couldn't open it: {reason}"),
    Err(e) => eprintln!("other error: {e}"),
}
```
