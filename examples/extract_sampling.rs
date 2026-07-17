//! The frame-sampling strategies and file-naming knob the main extract example doesn't cover.
//!
//! Shows: `Interval::Fps` and `Interval::EveryNFrames` (the main example uses
//! `EverySeconds`/`Count`/`Timestamps`), plus a custom `NamingScheme` written to a directory
//! via `output_dir`.
//!
//! Run with: `cargo run --example extract_sampling`

use media::prelude::*;

const INPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/video1.mp4");
const OUT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/temp_named");

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Sample at a fixed 2 frames per second, regardless of the source rate.
    let report = FrameExtractor::builder()
        .input(INPUT)
        .interval(Interval::Fps(2.0))
        .to_memory()
        .build()?
        .run()?;

    println!("Fps(2.0): {} frames", report.frame_count());
    for frame in report.frames() {
        println!("  frame {} @ {:?}", frame.index(), frame.timestamp());
    }

    // Take every 15th decoded frame.
    let report = FrameExtractor::builder()
        .input(INPUT)
        .interval(Interval::EveryNFrames(15))
        .to_memory()
        .build()?
        .run()?;

    println!("EveryNFrames(15): {} frames", report.frame_count());

    // Write to a directory with custom file names: `shot_000.jpg`, `shot_001.jpg`, …
    let report = FrameExtractor::builder()
        .input(INPUT)
        .interval(Interval::Count(3))
        .naming(NamingScheme::Sequential { prefix: "shot".into(), padding: 3 })
        .output_dir(OUT_DIR)
        .build()?
        .run()?;

    println!("Custom naming: wrote {} files (shot_000.jpg …) to {OUT_DIR}", report.frame_count());

    Ok(())
}
