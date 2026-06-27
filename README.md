# media-rs

Rust bindings to [FFmpeg](https://ffmpeg.org) — `libavcodec`, `libavformat`, `libavutil`, `libavfilter`, `libavdevice`, `libswscale`, `libswresample` and `libpostproc` — via **statically linked** libraries.

The crate links the prebuilt static FFmpeg binaries published at [`vegidio/binaries-ffmpeg`](https://github.com/vegidio/binaries-ffmpeg) and generates the raw FFI bindings with `bindgen` at build time. There is no system FFmpeg dependency and no `pkg-config` required.

> **Status:** early scaffold. Only the raw [`sys`] FFI bindings are exposed today; a safe, idiomatic Rust API is planned. The crate name is `media-rs` and the library is imported as `media`.

## Supported targets

Binaries are available for six platform/architecture combinations:

|         | x64 | arm64 |
|---------|-----|-------|
| macOS   | ✓   | ✓     |
| Linux   | ✓   | ✓     |
| Windows | ✓   | ✓     |

> Windows builds use the GNU toolchain (`*-pc-windows-gnu`); the static archives are GNU-style `.a` files.

The set of **bundled codecs and dependencies differs per target** (for example, x264/x265/vpx are not present on Windows arm64; hardware-acceleration backends such as NVENC, QSV and VAAPI vary by OS). The bindings cover the full portable FFmpeg API on every platform; the forthcoming Rust API will add guardrails that report when a feature is selected on a platform whose binaries don't provide it.

## How the build works

1. `build.rs` downloads `static_<os>_<arch>.zip` for the current target from the pinned `binaries-ffmpeg` release and caches it under `OUT_DIR`.
2. It discovers every `lib*.a` in the archive's `lib/` directory and links them all — the eight FFmpeg core libraries first (in dependency order), then the bundled third-party dependencies — using a `--start-group` link group on GNU linkers.

That's it — **no `bindgen`/`libclang` is required to build the crate.** The FFI bindings ([`src/sys/bindings.rs`](src/sys/bindings.rs)) are pre-generated and committed; see [Bindings](#bindings).

### Offline / custom builds

Set `MEDIA_BINARIES_DIR` to a directory containing `include/` and `lib/` subdirectories laid out like the release archives to skip the download and link against your own build:

```sh
MEDIA_BINARIES_DIR=/path/to/ffmpeg cargo build
```

## Bindings

The FFI bindings are **pre-generated and committed** at [`src/sys/bindings.rs`](src/sys/bindings.rs), so building the crate needs no `bindgen`/`libclang`. They are produced by the dev-only [`xtask`](xtask/) crate and the **"Generate bindings"** GitHub Actions workflow, not by `build.rs`.

Why committed rather than generated per build: the developer works on macOS but needs to reference hardware bindings that only the Windows/Linux SDKs can produce. FFmpeg's hardware headers come in two families — the `libavcodec/*` hwaccel headers (`videotoolbox.h`, `d3d11va.h`, `dxva2.h`, `vdpau.h`, `qsv.h`, `mediacodec.h`) and the `libavutil/hwcontext_*.h` headers — and each `#include`s an external OS/SDK header (`<d3d11.h>`, `<va/va.h>`, `<VideoToolbox/…>`, `<cuda.h>`, …) that only exists on the matching platform. So [`wrapper.h`](wrapper.h) gates each one on `__has_include(<its-sdk-header>)`, and the workflow runs `xtask generate` on a macOS + Linux + Windows matrix (each with its SDKs installed), then `xtask merge` unions the three into the committed file. Because bindgen output is pure Rust, every platform's structs are included **unconditionally** — you can reference `AVD3D11VADeviceContext` on macOS; the matching functions only fail to *link* if actually called on the wrong OS (the forthcoming Rust API will guard against that).

### Regenerating

Run the **"Generate bindings"** workflow from the repo's **Actions** tab ("Run workflow"), or with `gh workflow run bindings.yml`; it commits the refreshed `src/sys/bindings.rs` to `main`. For the host OS only, you can also run `cargo run -p xtask -- generate`.

> **libpostproc:** the current `binaries-ffmpeg` `26.6.0` static archives do **not** ship `libpostproc` (no `libpostproc.a`/headers), despite the upstream README listing it; the build skips it automatically and will pick it up if a later release includes it.

## License

GPL-3.0 — the linked FFmpeg binaries are built with `--enable-gpl`.