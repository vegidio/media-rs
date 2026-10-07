//! Encode on the machine's hardware H.264 encoder when it works, and on libx264 when it doesn't.
//!
//! Shows: `VideoEncoderBuilder::encoder` to name an encoder, and `probe` to test it before relying
//! on it.
//!
//! Run with: `cargo run --example hardware_encoder`

use media::codec::VideoEncoderBuilder;
use media::prelude::*;

/// The hardware encoders worth trying on this platform, best first.
const CANDIDATES: &[&str] = if cfg!(target_os = "macos") {
    &["h264_videotoolbox"]
} else {
    &["h264_nvenc", "h264_amf", "h264_qsv"]
};

fn h264() -> VideoEncoderBuilder {
    VideoEncoder::builder()
        .codec(VideoCodec::H264)
        .resolution(640, 360)
        .framerate(Framerate::fps(30))
        .gop_size(121)
        .option("bf", "0")
}

fn main() {
    for name in CANDIDATES {
        let hardware = h264().encoder(*name).pixel_format(PixelFormat::Nv12);
        match hardware.probe(24) {
            Ok(packets) => {
                let keyframes = packets.iter().filter(|p| p.is_keyframe()).count();
                println!("{name}: works, {} packets, {keyframes} keyframe(s)", packets.len());
                return;
            }
            Err(err) => println!("{name}: not used: {err}"),
        }
    }

    let packets = h264().option("sc_threshold", "0").probe(24).expect("libx264 is in every build");
    println!("libx264: {} packets", packets.len());
}
