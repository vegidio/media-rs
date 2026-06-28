//! Drop a stream during a transcode: keep only video, or keep only audio.
//!
//! Shows: the one-liner `.drop_audio()` and `.drop_video()` modifiers.
//!
//! Run with: `cargo run --example transcode_drop_streams`

use media::prelude::*;

// video2.mp4 is the asset that has both a video and an audio stream.
const INPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/video2.mp4");
const VIDEO_ONLY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/temp_video_only.mp4");
const AUDIO_ONLY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/temp_audio_only.mp4");

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Keep video, drop the audio track.
    transcode(INPUT).to(VIDEO_ONLY).drop_audio().run()?;
    println!("Wrote video-only file: {VIDEO_ONLY}");

    // Keep audio, drop the video track (the audio stream is copied through untouched).
    transcode(INPUT).to(AUDIO_ONLY).drop_video().run()?;
    println!("Wrote audio-only file: {AUDIO_ONLY}");

    Ok(())
}
