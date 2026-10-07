# Hardware encoders

**Use when:** you want to encode on the machine's GPU or media engine (VideoToolbox, NVENC, AMF,
Quick Sync) instead of the CPU, and fall back to the software encoder when that hardware isn't
there or doesn't work.

By default a codec encodes with its software encoder: `VideoCodec::H264` is `libx264`. A
hardware encoder is a different FFmpeg encoder for the same codec, so you name it with
`encoder(name)`. Then you find out whether it actually runs here with `probe`.

## The whole thing

```rust
use media::prelude::*;
use media::codec::VideoEncoderBuilder;

fn h264(width: u32, height: u32) -> VideoEncoderBuilder {
    VideoEncoder::builder()
        .codec(VideoCodec::H264)                     // (1)!
        .resolution(width, height)
        .framerate(Framerate::fps(30))
        .bitrate(Bitrate::mbps(6))
        .profile(H264Profile::High)
}

let hardware = h264(1920, 1080)
    .encoder("h264_videotoolbox")                    // (2)!
    .pixel_format(PixelFormat::Nv12)                 // (3)!
    .option("allow_sw", "0");                        // (4)!

let encoder = match hardware.clone().probe(24) {     // (5)!
    Ok(packets) if packets.first().is_some_and(Packet::is_keyframe) => hardware.build()?, // (6)!
    _ => h264(1920, 1080).preset(H264Preset::Veryfast).build()?, // (7)!
};
# Ok::<(), media::Error>(())
```

1. The codec stays H.264 whichever encoder you pick. It decides what the packets are, and what a
   muxer declares for the stream.
2. `encoder(name)` picks the FFmpeg encoder by name. It must encode the builder's codec:
   `encoder("libx265")` on an H.264 builder fails `build` with
   [`Error::InvalidConfig`](../reference/errors.md).
3. Hardware encoders usually take `nv12` (one luma plane, then the two chroma planes
   interleaved) rather than libx264's planar `yuv420p`. Feed it frames in that format, for
   example by ending a [filter chain](filters.md) in `format=nv12`.
4. Each encoder has its own options, set with `option`. VideoToolbox's `allow_sw=0` means
   hardware or nothing, so a Mac without a media engine fails here rather than quietly encoding
   in software.
5. `probe(24)` builds the encoder and encodes 24 generated frames with it, then flushes and
   returns the packets. The builder is `Clone`, so you probe one copy and keep the other to
   build from.
6. Anything you rely on can be checked on the packets: here, that the clip begins on a keyframe.
7. Any error falls back to the software encoder. Note the typed `preset` is set only here:
   VideoToolbox has no `preset`, and `build` fails with
   [`Error::UnknownOption("preset")`](../reference/errors.md) rather than drop it silently.

## Which builds ship which encoders

The prebuilt FFmpeg this crate links includes these H.264 encoders:

| Encoder | Hardware | Built for |
|---|---|---|
| `h264_videotoolbox` | Apple media engine | macOS |
| `h264_nvenc` | NVIDIA GPU | Linux x64/arm64, Windows x64 |
| `h264_amf` | AMD GPU | Linux x64/arm64, Windows x64/arm64 |
| `h264_qsv` | Intel Quick Sync | Linux x64, Windows x64 |
| `h264_vaapi` | VA-API | Linux |

An encoder missing from the build fails `build` with
[`Error::CodecUnavailable`](../reference/errors.md). Being in the build isn't enough, though:
NVENC, AMF and Quick Sync load the vendor's driver when they open, so on a machine without that
GPU or driver they fail then, with FFmpeg's error. That's what `probe` is for.

!!! note "VA-API needs hardware frames"
    `h264_vaapi` takes frames in GPU memory, uploaded into a hardware frames context. This crate
    doesn't create one yet, so `h264_vaapi` can't be used from it.

## Testing an encoder with `probe`

```rust
use media::prelude::*;

let packets = VideoEncoder::builder()
    .codec(VideoCodec::H264)
    .encoder("h264_nvenc")
    .resolution(640, 360)                            // (1)!
    .pixel_format(PixelFormat::Nv12)
    .framerate(Framerate::fps(30))
    .gop_size(121)
    .option("bf", "0")
    .probe(24)?;                                     // (2)!

let keyframes = packets.iter().filter(|p| p.is_keyframe()).count();
let reordered = packets.iter().any(|p| p.dts() != p.pts()); // (3)!
println!("{} packets, {keyframes} keyframes, reordered: {reordered}", packets.len());
# Ok::<(), media::Error>(())
```

1. A small clip is enough to find out whether the encoder runs, and costs milliseconds. Keep it
   above the hardware's minimum size; 640×360 is above every encoder's here.
2. The frames are generated at the configured size, pixel format and frame rate, one frame
   apart. The first half are one flat colour and the second half a different colour with a
   gradient: a hard scene cut halfway.
3. With `bf=0`, no frame is reordered, so every packet's `dts` equals its `pts`.

The scene cut lets you check how the encoder places keyframes. With a `gop_size` above the clip's
length, an encoder that keeps to it gives one keyframe, the first packet. One with scene-cut
detection on gives a second keyframe at the cut, as libx264 does unless you pass
`option("sc_threshold", "0")`.

`probe` fails as `build` and `encode` would: an encoder missing from the build, a driver that
isn't installed, a device that's busy, or an option the encoder doesn't take.

!!! warning "A probe is one encoder, at one moment"
    A passing probe says the encoder works now, alone. Some behaviour only shows under load.
    VideoToolbox, for example, adds keyframes of its own when several sessions encode at once,
    unless `option("realtime", "1")` is set. Probe with the options you'll encode with, and
    check what matters to you on the real output too.
