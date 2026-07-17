//! Transcode while reporting progress through a callback.
//!
//! Shows: `run_with_progress(|p| ...)` and the `Progress` snapshot (`percent`, `frames`,
//! `fps`, `processed_secs`, `total_secs`).
//!
//! Run with: `cargo run --example transcode_progress`

use media::prelude::*;

const INPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/video1.mp4");
const OUTPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/temp_progress.mp4");

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let job = Transcoder::builder().input(INPUT).output(OUTPUT).drop_audio().build()?;

    let summary = job.run_with_progress(|p| {
        // Overwrite the same line as progress advances.
        print!(
            "\rProgress: {:5.1}%  {:.2}/{:.2}s  {} frames  {:.0} fps   ",
            p.percent(),
            p.processed_secs(),
            p.total_secs(),
            p.frames(),
            p.fps()
        );
        use std::io::Write;
        let _ = std::io::stdout().flush();
    })?;

    println!("\nDone — wrote {OUTPUT} ({} frames, {:.2}s)", summary.frames, summary.duration_secs);
    Ok(())
}
