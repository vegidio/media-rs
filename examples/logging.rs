//! Control FFmpeg's log verbosity. FFmpeg is silenced by default (`Level::Quiet`); opt back
//! in to see messages from libavcodec / libx264 / etc.
//!
//! Shows: `media::log::set_level`, `media::log::level`, and the `Level` enum. You can also
//! set the verbosity without code via the `MEDIA_LOG` env var, e.g.:
//!
//!   MEDIA_LOG=debug cargo run --example logging
//!
//! An explicit `set_level` call always wins over `MEDIA_LOG`.
//!
//! Run with: `cargo run --example logging`

use media::prelude::*;

const INPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/video1.mp4");

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("Default level: {:?}", log::level());

    // Surface FFmpeg's own diagnostics for the work that follows.
    log::set_level(Level::Info);
    println!("Level is now: {:?}", log::level());

    // Any FFmpeg work after this point may print messages to stderr.
    let info = probe(INPUT)?;
    println!("Probed {INPUT}: {:.2}s", info.duration().as_secs_f64());

    // Quiet it again.
    log::set_level(Level::Quiet);
    println!("Level reset to: {:?}", log::level());

    Ok(())
}
