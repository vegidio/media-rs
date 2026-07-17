# Core concepts

A few ideas run through the whole API. Learn them once and every feature feels familiar.

## The three-tier API

Both **transcoding** and **frame extraction** expose the same three tiers. Pick the lowest
one that gives you the control you need.

=== "Tier 1 — one-liner"

    A fluent facade for the common case. Reads like a sentence; inherits sensible defaults.

    ```rust
    use media::prelude::*;
    transcode("input.mp4").to("output.webm").run()?;
    # Ok::<(), media::Error>(())
    ```

=== "Tier 2 — builder"

    Full option surface: codec, resolution, bit rate, trimming, filters, progress. The
    one-liner is just a thin wrapper over this.

    ```rust
    use media::prelude::*;
    let job = Transcoder::builder()
        .input("input.mp4")
        .output("output.mp4")
        .video(VideoConfig::builder().codec(VideoCodec::H264).build()?)
        .build()?;
    job.run()?;
    # Ok::<(), media::Error>(())
    ```

=== "Tier 3 — frame-level"

    Wire the pieces yourself: `MediaReader`, `Decoder`, `VideoEncoder`, `MediaWriter`. Total
    control, for when the high-level API can't express what you want.

    ```rust
    use media::prelude::*;
    let mut reader = MediaReader::open("input.mp4")?;
    let vidx = reader.best_stream(StreamKind::Video)?;
    let mut decoder = reader.stream(vidx).decoder()?;
    for packet in reader.packets() {
        let packet = packet?;
        if packet.stream_index() != vidx { continue; }
        for frame in decoder.decode(&packet)? {
            let _frame = frame?; // do something with the decoded frame
        }
    }
    # Ok::<(), media::Error>(())
    ```

The builders follow a consistent shape: `X::builder()` → chained by-value setters →
`.build()` (which validates required fields) → `.run()`.

## Time is `Duration`, rates are `f64`

The API distinguishes **points/spans of time** from **rates**:

- **Operational time inputs** use [`std::time::Duration`] — e.g. a trim range
  (`Duration::from_secs(1)..=Duration::from_secs(3)`), an extraction `range`, or
  `Interval::Timestamps`.
- **Rates** stay `f64` — e.g. `Interval::EverySeconds(0.5)` and `Interval::Fps(2.0)`.

  [`std::time::Duration`]: https://doc.rust-lang.org/std/time/struct.Duration.html

```rust
use media::prelude::*;
use std::time::Duration;

transcode("in.mp4")
    .to("out.mp4")
    .trim(Duration::from_secs(1)..=Duration::from_secs(3)) // a span → Duration
    .run()?;
# Ok::<(), media::Error>(())
```

## Progress is shared

Transcoding and extraction both report progress through the **same**
[`Progress`](../reference/transcode.md#progress) type via `run_with_progress(|p| …)`:

```rust
use media::prelude::*;
# fn demo(job: media::Transcoder) -> media::Result<()> {
job.run_with_progress(|p| {
    println!("{:.1}% — {} frames @ {:.0} fps", p.percent(), p.frames(), p.fps());
})?;
# Ok(()) }
```

## FFmpeg is quiet by default

FFmpeg's libraries normally write diagnostics straight to stderr. `media-rs` **silences them
by default** ([`Level::Quiet`]). Opt back in when you need to debug:

```rust
media::log::set_level(media::Level::Info); // or set MEDIA_LOG=info in the environment
```

  [`Level::Quiet`]: ../guides/logging.md

See the [Logging guide](../guides/logging.md) for the full level list and the `MEDIA_LOG`
environment variable.

## Errors

Fallible calls return [`Result<T>`](../reference/errors.md) (an alias for
`Result<T, media::Error>`). Most FFmpeg failures surface as `Error::Internal { code, message }`
with FFmpeg's decoded message; the crate raises more specific variants (`NoVideoStream`,
`UnsupportedResolution`, `InvalidConfig`, …) when it can say something more actionable. See
the [Errors reference](../reference/errors.md).
