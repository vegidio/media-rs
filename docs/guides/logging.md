# Logging

**Use when:** you want to see (or silence) FFmpeg's own diagnostics — messages from
`libavcodec`, `libx264`, and friends.

FFmpeg writes these straight to stderr. Left at FFmpeg's default level they're noisy, so
`media-rs` **silences FFmpeg by default** ([`Level::Quiet`]). You opt back in when debugging.

  [`Level::Quiet`]: ../reference/logging.md

## Setting the level in code

```rust
use media::prelude::*;

println!("default: {:?}", log::level());  // (1)!

log::set_level(Level::Info);              // (2)!
println!("now: {:?}", log::level());

let info = probe("input.mp4")?;           // (3)!
println!("{:.2}s", info.duration().as_secs_f64());

log::set_level(Level::Quiet);             // (4)!
# Ok::<(), media::Error>(())
```

1. `log::level()` reads the current verbosity. `log` is re-exported by the prelude, so
   `log::…` refers to `media::log`, not any other logging crate you might have.
2. `log::set_level(Level::Info)` raises verbosity. From here on, FFmpeg work may print to
   stderr.
3. Any FFmpeg operation after `set_level` (like this `probe`) can now emit messages.
4. Set it back to `Level::Quiet` to silence FFmpeg again. The level is process-global.

## Setting the level without code

Set the `MEDIA_LOG` environment variable to any level name (case-insensitive):

```sh
MEDIA_LOG=debug cargo run --example logging
```

The default level is applied lazily the first time the crate does any FFmpeg work, reading
`MEDIA_LOG` at that point.

!!! note "An explicit call always wins"
    A `log::set_level(...)` call **overrides** `MEDIA_LOG`. Use the environment variable for
    ad-hoc debugging, and `set_level` when your program needs deterministic control.

## Levels

From most to least restrictive (each includes everything above it):

| `Level` | Shows |
|---------|-------|
| `Quiet` | Nothing (**crate default**). |
| `Panic` | Unrecoverable, process-ending failures. |
| `Fatal` | Unrecoverable errors. |
| `Error` | Errors after which processing may continue. |
| `Warning` | Likely-incorrect or unexpected situations. |
| `Info` | Standard informational output (FFmpeg's own default). |
| `Verbose` | Verbose informational output. |
| `Debug` | Debugging output. |
| `Trace` | Extremely verbose tracing. |

Accepted `MEDIA_LOG` values are those names in lowercase (`warn` is also accepted for
`warning`).

## See also

- [Logging API reference](../reference/logging.md)
