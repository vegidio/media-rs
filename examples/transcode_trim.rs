//! Keep only a time range of the input (an inclusive range of [`Duration`]s).
//!
//! Shows: the one-liner `.trim(start..=end)` modifier.
//!
//! Run with: `cargo run --example transcode_trim`

use media::prelude::*;
use std::time::Duration;

const INPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/video1.mp4");
const OUTPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/temp_trim.mp4");

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Keep seconds 1 through 3 of the input.
    let summary = transcode(INPUT).to(OUTPUT).trim(Duration::from_secs(1)..=Duration::from_secs(3)).run()?;

    println!("Wrote {OUTPUT}\n  {} frames, {:.2}s", summary.frames, summary.duration_secs);
    Ok(())
}
