# Installation

## Add the crate

Add `media-rs` to your project (the library is imported as `media`):

```sh
cargo add media-rs
```

Or add it to `Cargo.toml` by hand:

```toml
[dependencies]
media-rs = "26"
```

Then import the prelude, which re-exports everything most code needs:

```rust
use media::prelude::*;
```

## What happens at build time

There is **no system FFmpeg dependency** and **no `bindgen`/`libclang`** requirement:

1. `build.rs` downloads a prebuilt static FFmpeg archive for your target from
   [`vegidio/binaries-ffmpeg`](https://github.com/vegidio/binaries-ffmpeg) and caches it.
2. It links every static library in the archive, in dependency order.
3. The raw FFI bindings are already committed in the crate (`src/sys/bindings.rs`), so nothing
   is generated on your machine.

You just need a Rust toolchain and a network connection on the first build.

## Supported targets

Prebuilt binaries are available for six platform/architecture combinations:

|         | x64 | arm64 |
|---------|:---:|:-----:|
| macOS   | ✓  |  ✓   |
| Linux   | ✓  |  ✓   |
| Windows | ✓  |  ✓   |

!!! note "Windows uses MSVC"
    Windows builds use the **MSVC** toolchain (`*-pc-windows-msvc`); the static archives are
    MSVC `.lib` files.

The set of **bundled codecs differs per target** (for example, some hardware encoders ship on
one platform but not another). The portable FFmpeg API is available everywhere.

## Offline / custom builds

To skip the download and link against your own FFmpeg build, point `MEDIA_BINARIES_DIR` at a
directory that contains `include/` and `lib/` laid out like the release archives:

```sh
MEDIA_BINARIES_DIR=/path/to/ffmpeg cargo build
```

## License

`media-rs` is **Apache 2.0**. Make sure that it is compatible with your project before depending on it.
