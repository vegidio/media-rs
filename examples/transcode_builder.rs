//! Full control over the output video via the builder API.
//!
//! Shows: Tier 2 `Transcoder::builder()` plus `VideoConfig::builder()` with an explicit
//! codec, resolution, bit rate, frame rate, preset and profile.
//!
//! Run with: `cargo run --example transcode_builder`

use media::prelude::*;

const INPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/video1.mp4");
const OUTPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/temp_builder.mp4");

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let video = VideoConfig::builder()
        .codec(VideoCodec::H264)
        .resolution(640, 360)
        .bitrate(Bitrate::mbps(2))
        .framerate(Framerate::fps(30))
        .preset(H264Preset::Fast)
        .profile(H264Profile::High)
        .build()?;

    let job = Transcoder::builder()
        .input(INPUT)
        .output(OUTPUT)
        .video(video)
        .drop_audio()
        .build()?;

    let summary = job.run()?;

    println!(
        "Wrote {OUTPUT}\n  {} frames, {:.2}s (640x360 H.264)",
        summary.frames, summary.duration_secs
    );
    Ok(())
}
