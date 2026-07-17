//! Applying an audio filter chain during a transcode.
//!
//! Shows: `AudioFilterChain` built from typed operators (high-pass/low-pass, gain), plus the
//! `.raw()` escape hatch for arbitrary libavfilter strings. Setting an audio filter forces the
//! audio to be re-encoded.
//!
//! Run with: `cargo run --example audio_filters`

use media::prelude::*;

const INPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/audio1.mp3");
const OUTPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/temp_filtered.m4a");

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Remove low rumble and harsh highs, then bump the level by 3 dB.
    let chain = AudioFilterChain::new().highpass(80.0).lowpass(15_000.0).gain(Decibels(3.0));

    transcode(INPUT).to(OUTPUT).audio_filter(chain).run()?;
    println!("Wrote {OUTPUT}");

    // For anything not covered by a typed operator, drop to a raw filtergraph string.
    let _raw = AudioFilterChain::raw("loudnorm=I=-16:TP=-1.5:LRA=11");

    Ok(())
}
