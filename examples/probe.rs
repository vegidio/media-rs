//! Inspect a media file's container and stream metadata without decoding any frames.
//!
//! Shows: `probe()`, `MediaInfo` (`format_name`, `duration`, `streams`, `video`, `audio`) and
//! the `StreamInfo` fields, including the codec name and RFC 6381 codec string.
//!
//! Run with: `cargo run --example probe`

use media::prelude::*;

const INPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/video1.mp4");

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let info = probe(INPUT)?;

    println!("File: {INPUT}");
    println!("Container: {}", info.format_name());
    println!("Duration: {:.2}s", info.duration().as_secs_f64());
    println!("Streams: {}", info.stream_count());

    // Every stream, with the fields that apply to its kind.
    for stream in info.streams() {
        match stream.kind {
            StreamKind::Video => println!(
                "  [{}] {:?}: {}x{}, codec {:?}",
                stream.index, stream.kind, stream.width, stream.height, stream.video_codec
            ),
            StreamKind::Audio => println!(
                "  [{}] {:?}: {} Hz, codec {:?}",
                stream.index, stream.kind, stream.sample_rate, stream.audio_codec
            ),
            other => println!("  [{}] {:?}", stream.index, other),
        }
        // FFmpeg's name for any codec, and the string a MIME type's `codecs=` parameter wants (if FFmpeg has one).
        println!("      {} / {}", stream.codec_name, stream.codec_string.as_deref().unwrap_or("no codec string"));
    }

    // Convenience accessors for the first video / audio stream.
    if let Some(video) = info.video() {
        println!("First video stream: {}x{}", video.width, video.height);
    }
    if let Some(audio) = info.audio() {
        println!("First audio stream: {} Hz", audio.sample_rate);
    }

    Ok(())
}
