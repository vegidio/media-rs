# Logging

Module `media::log`. FFmpeg log verbosity control. See the [Logging guide](../guides/logging.md).

FFmpeg is **silenced by default** ([`Level::Quiet`](#level)). The default is applied lazily the
first time the crate does any FFmpeg work.

## Functions

```rust
pub fn set_level(level: Level)  // set verbosity; overrides the default and MEDIA_LOG
pub fn level() -> Level         // the current verbosity
```

An explicit `set_level` always wins over the `MEDIA_LOG` environment variable.

## `Level`

FFmpeg log verbosity, most to least restrictive (each includes everything above it).

```rust
pub enum Level {
    Quiet,   // nothing — the crate default
    Panic,   // process-ending failures
    Fatal,   // unrecoverable errors
    Error,   // errors after which processing may continue
    Warning, // likely-incorrect / unexpected situations
    Info,    // standard informational output (FFmpeg's own default)
    Verbose, // verbose informational output
    Debug,   // debugging output
    Trace,   // extremely verbose tracing
}
```

## `MEDIA_LOG`

Set the startup level without code. Accepts the `Level` names, case-insensitive (`warn` is
also accepted for `warning`):

```sh
MEDIA_LOG=debug cargo run
```
