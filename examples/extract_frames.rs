//! Extract still images from a video across the three tiers of the API.
//!
//! Shows: the `extract_frames` one-liner, the `FrameExtractor` builder (interval, format,
//! resolution, in-memory + callback outputs, range, progress), and the Tier-3
//! `stream().sampled_at()` iterator.
//!
//! Run with: `cargo run --example extract_frames`

use media::prelude::*;
use std::time::Duration;

const INPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/video1.mp4");
const OUT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/temp_frames");

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // --- Tier 1: one JPEG per second into a directory ---------------------------------------
    let report = extract_frames(INPUT)
        .every(Duration::from_secs(1))
        .to_dir(OUT_DIR)
        .run()?;
    println!(
        "Tier 1: wrote {} frames to {OUT_DIR} in {:?}",
        report.frame_count(),
        report.elapsed()
    );

    // --- Tier 2: exactly 5 PNGs evenly spread, scaled, kept in memory -----------------------
    let report = FrameExtractor::builder()
        .input(INPUT)
        .interval(Interval::Count(5))
        .format(ImageFormat::Png)
        .resolution(Resolution::Fixed(320, 180))
        .to_memory()
        .build()?
        .run()?;
    println!("Tier 2: {} in-memory frames:", report.frame_count());
    for frame in report.frames() {
        let (w, h) = frame.dimensions();
        println!("  frame {} @ {:?} — {w}x{h}", frame.index(), frame.timestamp());
    }

    // --- Tier 2: exact timestamps, streamed out via a callback ------------------------------
    FrameExtractor::builder()
        .input(INPUT)
        .interval(Interval::Timestamps(vec![
            Duration::from_millis(500),
            Duration::from_millis(1_500),
        ]))
        .to_callback(|frame| {
            let jpeg = frame.encode(ImageFormat::Jpeg { quality: 85 })?;
            println!(
                "Callback: frame {} @ {:?} → {} JPEG bytes",
                frame.index(),
                frame.timestamp(),
                jpeg.len()
            );
            Ok(())
        })
        .build()?
        .run()?;

    // --- Tier 2: a range + progress reporting -----------------------------------------------
    FrameExtractor::builder()
        .input(INPUT)
        .interval(Interval::EverySeconds(0.5))
        .range(Duration::from_secs(1)..=Duration::from_secs(2))
        .to_memory()
        .build()?
        .run_with_progress(|p| {
            println!("Progress: {} frames ({:.0}%)", p.frames(), p.percent());
        })?;

    // --- Tier 3: iterate frames yourself, save the ones you keep ----------------------------
    let mut reader = MediaReader::open(INPUT)?;
    let vidx = reader.best_stream(StreamKind::Video)?;
    let mut kept = 0;
    for frame in reader.stream(vidx).sampled_at(Interval::Count(3))? {
        let frame = frame?;
        let (w, h) = frame.dimensions();
        println!("Tier 3: frame {} is {w}x{h}", frame.index());
        kept += 1;
    }
    println!("Tier 3: iterated {kept} frames");

    Ok(())
}
