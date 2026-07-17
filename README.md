# media-rs

A Rust library to encode, decode and process audio/video using [FFmpeg](https://ffmpeg.org), via statically linked libraries.

The crate links the prebuilt static FFmpeg binaries published at [`vegidio/binaries-ffmpeg`](https://github.com/vegidio/binaries-ffmpeg) and ships **pre-generated, committed** raw FFI bindings — no build-time `bindgen`/`libclang`, no system FFmpeg dependency and no `pkg-config` required.

## ⬇️ Installation

This library can be installed using Cargo. To do that, run the following command in your project's root directory:

```bash
cargo add media-rs
```

The crate links as `media`, so you import it with `use media;` regardless of the package name.

> [!NOTE]
> The first build downloads the prebuilt static binaries for your platform, so an internet connection is required (see [Troubleshooting](#-troubleshooting) for offline builds).

### Supported targets

Binaries are available for six platform/architecture combinations:

|         | x64 | arm64 |
|---------|-----|-------|
| macOS   | ✓  | ✓    |
| Linux   | ✓  | ✓    |
| Windows | ✓  | ✓    |

> Windows builds use the **MSVC** toolchain (`*-pc-windows-msvc`). The set of bundled codecs and dependencies differs per target (for example, NVENC and QSV ship on Windows x64 but not arm64; VAAPI is Linux-only).

## 🤖 Usage

Here are some examples of how to work with media files using this library. These snippets don't have any error handling for the sake of simplicity, but you should always check for errors in production code.

> [!TIP]
> The complete documentation, with a detailed explanation of how each API works, is available at **[vegidio.github.io/media-rs](https://vegidio.github.io/media-rs/)**.

#### Probing

Inspect a file's container and stream metadata without decoding any frames:

```rust
use media::prelude::*;

let info = probe("/path/to/video.mp4").unwrap();
println!("Duration: {:.2}s", info.duration().as_secs_f64());

if let Some(video) = info.video() {
    println!("Video: {}x{}", video.width, video.height);
}
```

#### Transcoding

The one-liner transcode infers the output container and codecs from the file extension:

```rust
use media::prelude::*;

let summary = transcode("/path/to/input.mp4").to("/path/to/output.webm").run().unwrap();
println!("Wrote {} frames in {:.2}s", summary.frames, summary.duration_secs);
```

#### Transcoding with custom settings

Use the builder to take full control over the output video:

```rust
use media::prelude::*;

let video = VideoConfig::builder()
    .codec(VideoCodec::H264)
    .resolution(640, 360)
    .bitrate(Bitrate::mbps(2))
    .framerate(Framerate::fps(30))
    .preset(H264Preset::Fast)
    .profile(H264Profile::High)
    .build()
    .unwrap();

let summary = Transcoder::builder()
    .input("/path/to/input.mp4")
    .output("/path/to/output.mp4")
    .video(video)
    .drop_audio()
    .build()
    .unwrap()
    .run()
    .unwrap();
```

#### Extracting frames

Extract still images from a video — here, one JPEG per second into a directory:

```rust
use media::prelude::*;
use std::time::Duration;

let report = extract_frames("/path/to/video.mp4")
    .every(Duration::from_secs(1))
    .to_dir("/path/to/frames")
    .run()
    .unwrap();

println!("Wrote {} frames in {:?}", report.frame_count(), report.elapsed());
```

#### Remuxing

Change a file's container without re-encoding, by copying every stream through verbatim:

```rust
use media::prelude::*;

let mut reader = MediaReader::open("/path/to/input.mp4").unwrap();
let mut writer = MediaWriter::create("/path/to/output.mkv").unwrap();

let mut out_index = Vec::new();
for i in 0..reader.stream_count() {
    out_index.push(writer.add_stream_copy(&reader, i).unwrap());
}

writer.write_header().unwrap();
for packet in reader.packets() {
    let mut packet = packet.unwrap();
    packet.set_stream_index(out_index[packet.stream_index()]);
    writer.write_packet(&mut packet).unwrap();
}
writer.write_trailer().unwrap();
```

#### Runnable examples

The [`examples/`](examples) directory has standalone programs covering each part of the API, runnable out of the box against the bundled assets:

```bash
cargo run --example probe                    # inspect streams without decoding
cargo run --example transcode_oneliner       # transcode(input).to(output).run()
cargo run --example transcode_builder        # full VideoConfig builder (H.264)
cargo run --example transcode_codecs         # H.265 / VP9 / AV1 into matching containers
cargo run --example transcode_drop_streams   # keep only video / only audio
cargo run --example transcode_trim           # keep a time range
cargo run --example transcode_progress       # progress callback
cargo run --example filters                  # FilterChain (scale/fps/denoise/color)
cargo run --example remux                    # stream-copy to a new container (no re-encode)
cargo run --example extract_frames           # extract stills across all three tiers
cargo run --example extract_sampling         # Fps / EveryNFrames intervals + custom naming
cargo run --example extract_save             # save frames + raw RGB pixel access
cargo run --example decode_frames            # MediaReader + Decoder (Tier 3 read)
cargo run --example seek                     # seek to a timestamp before decoding
cargo run --example transcode_lowlevel       # read → decode → encode → mux by hand
cargo run --example logging                  # FFmpeg log verbosity control
cargo run --example raw_ffi                  # media::sys raw FFI escape hatch
cargo run --example version                  # print the linked FFmpeg version
```

They read from `assets/video*.mp4` and write outputs to `assets/temp_*` (git-ignored).

## 💣 Troubleshooting

### My build fails because it can't download the binaries

The first build fetches the prebuilt static libraries for your platform over the network. For offline or air-gapped builds, provide a directory containing `include/` and `lib/` subdirectories laid out like the release archives, and point the build at it with the `MEDIA_BINARIES_DIR` environment variable:

```bash
MEDIA_BINARIES_DIR=/path/to/extracted/libs cargo build
```

## 📝 License

**media-rs** is released under the Apache 2.0 License. See [LICENSE](LICENSE) for details.

## 👨🏾‍💻 Author

Vinicius Egidio ([vinicius.io](http://vinicius.io))