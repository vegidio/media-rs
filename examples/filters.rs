//! Apply a chain of video filters during a transcode.
//!
//! Shows: `VideoFilterChain` built from typed operators (`scale`, `fps`, `denoise` with
//! `DenoiseLevel`, `color_correct` with `ColorCorrect`), plus the `raw` escape hatch for an
//! arbitrary libavfilter string.
//!
//! Run with: `cargo run --example filters`

use media::prelude::*;

const INPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/video1.mp4");
const OUTPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/temp_filtered.mp4");

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Typed, composable filter chain: scale -> force fps -> denoise -> color correct.
    let chain = VideoFilterChain::new()
        .scale(640, 360)
        .fps(24)
        .denoise(DenoiseLevel::Moderate)
        .color_correct(|cc| cc.brightness(0.05).contrast(1.1).saturation(1.2));

    println!("Filter graph: {}", chain.description());

    // For anything not covered by the typed builders, drop down to a raw filter string:
    let _raw = VideoFilterChain::raw("scale=1280:720,unsharp=5:5:1.0");

    let summary = transcode(INPUT).to(OUTPUT).drop_audio().video_filter(chain).run()?;

    println!("Wrote {OUTPUT}\n  {} frames, {:.2}s", summary.frames, summary.duration_secs);
    Ok(())
}
