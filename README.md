# media-rs

Rust bindings to [FFmpeg](https://ffmpeg.org) — `libavcodec`, `libavformat`, `libavutil`, `libavfilter`, `libavdevice`, `libswscale`, `libswresample` and `libpostproc` — via **statically linked** libraries.

The crate links the prebuilt static FFmpeg binaries published at [`vegidio/binaries-ffmpeg`](https://github.com/vegidio/binaries-ffmpeg) and ships **pre-generated, committed** raw FFI bindings — no build-time `bindgen`/`libclang`, no system FFmpeg dependency and no `pkg-config` required.

> **Status:** a safe, idiomatic Rust API built on the raw [`sys`] bindings is available — reading/probing media, decoding, video encoding, filtering, and transcoding — with the common entry points re-exported from `media::prelude`. Still missing: audio re-encoding, an `async` feature, and per-platform hardware guardrails. The crate name is `media-rs` and the library is imported as `media`.

## Supported targets

Binaries are available for six platform/architecture combinations:

|         | x64 | arm64 |
|---------|-----|-------|
| macOS   | ✓   | ✓     |
| Linux   | ✓   | ✓     |
| Windows | ✓   | ✓     |

> Windows builds use the **MSVC** toolchain (`*-pc-windows-msvc`); the static archives are MSVC `.lib` files (vcpkg `*-static-md`: static libraries, dynamic UCRT).

The set of **bundled codecs and dependencies differs per target** (for example, NVENC, QSV and OpenGL ship on Windows x64 but not Windows arm64; VAAPI is Linux-only). The bindings cover the full portable FFmpeg API on every platform; the forthcoming Rust API will add guardrails that report when a feature is selected on a platform whose binaries don't provide it.

## How the build works

1. `build.rs` downloads `static_<os>_<arch>.zip` for the current target from the pinned `binaries-ffmpeg` release and caches it under `OUT_DIR`.
2. It discovers and links every static library in the archive's `lib/` directory (`lib*.a` on macOS/Linux, `*.lib` on MSVC Windows) — FFmpeg core libraries first in dependency order, then the bundled third-party deps. GNU linkers wrap the set in a `--start-group` link group to resolve circular refs; the multi-pass macOS `ld64` and Windows `link.exe` need no group.

That's it — **no `bindgen`/`libclang` is required to build the crate.** The FFI bindings ([`src/sys/bindings.rs`](src/sys/bindings.rs)) are pre-generated and committed; see [Bindings](#bindings).

### Offline / custom builds

Set `MEDIA_BINARIES_DIR` to a directory containing `include/` and `lib/` subdirectories laid out like the release archives to skip the download and link against your own build:

```sh
MEDIA_BINARIES_DIR=/path/to/ffmpeg cargo build
```

## Bindings

The FFI bindings are **pre-generated and committed** at [`src/sys/bindings.rs`](src/sys/bindings.rs), so building the crate needs no `bindgen`/`libclang`. They are produced by the dev-only [`xtask`](xtask/) crate and the **"Generate bindings"** GitHub Actions workflow, not by `build.rs`.

Why committed rather than generated per build: no single machine has every hardware SDK (e.g. a macOS dev can't produce the Windows/Linux hwaccel bindings). FFmpeg's hardware headers come in two families — the `libavcodec/*` hwaccel headers and the `libavutil/hwcontext_*.h` headers — and each `#include`s an external OS/SDK header (`<d3d11.h>`, `<va/va.h>`, `<VideoToolbox/…>`, `<cuda.h>`, …) that exists only on the matching platform. So [`wrapper.h`](wrapper.h) gates each block on `__has_include(<its-sdk-header>)`; the workflow runs `xtask generate` on a macOS + Linux + Windows matrix (each with its SDKs installed), then `xtask merge` unions the three. Because bindgen output is pure Rust, every platform's structs are included **unconditionally** — you can reference `AVD3D11VADeviceContext` on macOS; the matching functions only fail to *link* if actually called on the wrong OS (the forthcoming guardrails will prevent that).

### Regenerating

Run the **"Generate bindings"** workflow from the repo's **Actions** tab ("Run workflow"), or with `gh workflow run bindings.yml`; it commits the refreshed `src/sys/bindings.rs` to `main`. For the host OS only, you can also run `cargo run -p xtask -- generate`.

> **libpostproc:** the current `binaries-ffmpeg` `26.6.3` static archives do **not** ship `libpostproc` (no `libpostproc.a`/headers), despite the upstream README listing it; the build skips it automatically and will pick it up if a later release includes it.

## Examples

Runnable, single-feature examples live in [`examples/`](examples/) — one per public capability, from the one-liner `transcode()` up to the frame-level pipeline. Run any of them with:

```sh
cargo run --example version              # linked FFmpeg version
cargo run --example raw_ffi              # media::sys raw FFI escape hatch
cargo run --example probe                # inspect streams without decoding
cargo run --example logging              # FFmpeg log verbosity control
cargo run --example transcode_oneliner   # transcode(input).to(output).run()
cargo run --example transcode_drop_streams   # keep only video / only audio
cargo run --example transcode_trim       # keep a time range
cargo run --example transcode_builder    # full VideoConfig builder (H.264)
cargo run --example transcode_codecs     # H.265 / VP9 / AV1 into matching containers
cargo run --example transcode_progress   # progress callback
cargo run --example filters              # FilterChain (scale/fps/denoise/color)
cargo run --example remux                # stream-copy to a new container (no re-encode)
cargo run --example extract_frames       # extract stills across all three tiers
cargo run --example extract_sampling     # Fps / EveryNFrames intervals + custom naming
cargo run --example extract_save         # save frames + raw RGB pixel access
cargo run --example decode_frames        # MediaReader + Decoder (Tier 3 read)
cargo run --example seek                 # seek to a timestamp before decoding
cargo run --example transcode_lowlevel   # read → decode → encode → mux by hand
```

They read from `assets/video*.mp4` and write outputs to `assets/temp_*` (git-ignored).

## Testing & coverage

Run the test suite with `cargo test`. It includes unit tests plus integration tests under [`tests/`](tests/) that exercise `assets/video*.mp4`; the integration tests skip cleanly when those assets are absent.

Code coverage is measured with [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov). Generate a report locally with the helper script (it installs the tooling on first run):

```sh
scripts/coverage.sh          # build and open an HTML report
scripts/coverage.sh --lcov   # emit target/coverage/lcov.info instead
```

Equivalent cargo aliases are configured in [`.cargo/config.toml`](.cargo/config.toml): `cargo cov` and `cargo cov-lcov`. Coverage is a local-only dev tool — there is no coverage CI workflow.

## License

GPL-3.0 — the linked FFmpeg binaries are built with `--enable-gpl`.