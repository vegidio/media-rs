# media-rs

Rust bindings to [FFmpeg](https://ffmpeg.org) — `libavcodec`, `libavformat`, `libavutil`,
`libavfilter`, `libavdevice`, `libswscale`, `libswresample` and `libpostproc` — via
**statically linked** libraries.

The crate links the prebuilt static FFmpeg binaries published at
[`vegidio/binaries-ffmpeg`](https://github.com/vegidio/binaries-ffmpeg) and generates the
raw FFI bindings with `bindgen` at build time. There is no system FFmpeg dependency and no
`pkg-config` required.

> **Status:** early scaffold. Only the raw [`sys`] FFI bindings are exposed today; a safe,
> idiomatic Rust API is planned. The crate name is `media-rs` and the library is imported
> as `media`.

## Supported targets

Binaries are available for six platform/architecture combinations:

|         | x64 | arm64 |
|---------|-----|-------|
| macOS   | ✓   | ✓     |
| Linux   | ✓   | ✓     |
| Windows | ✓   | ✓     |

> Windows builds use the GNU toolchain (`*-pc-windows-gnu`); the static archives are
> GNU-style `.a` files.

The set of **bundled codecs and dependencies differs per target** (for example,
x264/x265/vpx are not present on Windows arm64; hardware-acceleration backends such as
NVENC, QSV and VAAPI vary by OS). The bindings cover the full portable FFmpeg API on every
platform; the forthcoming Rust API will add guardrails that report when a feature is
selected on a platform whose binaries don't provide it.

## How the build works

1. `build.rs` downloads `static_<os>_<arch>.zip` for the current target from the pinned
   `binaries-ffmpeg` release and caches it under `OUT_DIR`.
2. It discovers every `lib*.a` in the archive's `lib/` directory and links them all —
   the eight FFmpeg core libraries first (in dependency order), then the bundled
   third-party dependencies — using a `--start-group` link group on GNU linkers.
3. It runs `bindgen` over [`wrapper.h`](wrapper.h) to generate the FFI bindings into
   `OUT_DIR/bindings.rs`, included by [`src/sys.rs`](src/sys.rs).

### Offline / custom builds

Set `MEDIA_BINARIES_DIR` to a directory containing `include/` and `lib/` subdirectories
laid out like the release archives to skip the download and link against your own build:

```sh
MEDIA_BINARIES_DIR=/path/to/ffmpeg cargo build
```

## Hardware-context bindings

FFmpeg's hardware headers come in two families — the `libavcodec/*` hwaccel headers
(`videotoolbox.h`, `d3d11va.h`, `dxva2.h`, `vdpau.h`, `qsv.h`, `mediacodec.h`) and the
`libavutil/hwcontext_*.h` headers. The archives ship all of them on every platform, but
each `#include`s an external OS/SDK header (`<d3d11.h>`, `<va/va.h>`,
`<VideoToolbox/…>`, `<cuda.h>`, …) that only exists on the matching platform. So
[`wrapper.h`](wrapper.h) gates each one on `__has_include(<its-sdk-header>)`: bindgen binds
exactly the backends the build host can parse. On macOS that means the self-contained
backends (DRM, MediaCodec, OpenHarmony), Vulkan, and VideoToolbox; per-OS CI runners pick
up D3D, VA-API, VDPAU, CUDA, QSV, OpenCL and AMF. The generic device-type and pixel-format
enums for **every** backend are part of the portable core and are always present, so any
backend can be *named* on any platform — only the SDK-specific device-context structs are
host-gated, and a future per-OS CI step will commit the full union.

> **Note:** the current `binaries-ffmpeg` `26.6.0` static archives do **not** ship
> `libpostproc` (no `libpostproc.a`/headers), despite the upstream README listing it; the
> build skips it automatically and will pick it up if a later release includes it.

## License

GPL-3.0 — the linked FFmpeg binaries are built with `--enable-gpl`.
