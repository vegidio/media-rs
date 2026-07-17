//! The simplest possible transcode: one chained call. The output container/codecs are
//! inferred from the output extension.
//!
//! Shows: Tier 1 `transcode(input).to(output).run()` and the `TranscodeSummary` it returns.
//!
//! Run with: `cargo run --example transcode_oneliner`

use media::prelude::*;

const INPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/video1.mp4");
const OUTPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/temp_oneliner.mp4");

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let summary = transcode(INPUT).to(OUTPUT).run()?;

    println!("Wrote {OUTPUT}\n  {} frames, {:.2}s", summary.frames, summary.duration_secs);
    Ok(())
}
