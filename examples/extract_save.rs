//! Extract frames into memory, inspect their raw pixels, and save them straight to disk.
//!
//! Shows: `ExtractReport::into_frames` (take ownership of the buffered frames),
//! `ExtractedFrame::to_rgb_bytes` (borrow the packed RGB pixels) and
//! `ExtractedFrame::save` (write to a path, format inferred from the extension).
//!
//! Run with: `cargo run --example extract_save`

use media::prelude::*;

const INPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/video1.mp4");
const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/");

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let report = FrameExtractor::builder().input(INPUT).interval(Interval::Count(3)).to_memory().build()?.run()?;

    // Take ownership of the in-memory frames so we can move them around freely.
    for frame in report.into_frames() {
        let (w, h) = frame.dimensions();
        // Raw pixel access: tightly packed RGB, `w * h * 3` bytes.
        let bytes = frame.to_rgb_bytes();
        let path = format!("{DIR}temp_frame_{}.png", frame.index());

        // Save straight to disk; the `.png` extension selects the encoder.
        frame.save(&path)?;

        println!(
            "frame {} @ {:?} — {w}x{h}, {} raw RGB bytes -> {path}",
            frame.index(),
            frame.timestamp(),
            bytes.len()
        );
    }

    Ok(())
}
