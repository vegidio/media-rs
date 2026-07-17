//! Transcode to codecs other than the default H.264, into matching containers.
//!
//! Shows: selecting `VideoCodec::H265`, `Vp9` and `Av1` via `VideoConfig::builder()`; the
//! output container is inferred from the file extension (`.mp4`, `.webm`, `.mkv`). Unlike
//! the H.264 builder example, `preset`/`profile` are omitted here — those are H.264-specific.
//!
//! Run with: `cargo run --example transcode_codecs`

use media::prelude::*;

const INPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/video1.mp4");
const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/");

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // (codec, output file) — the extension decides the container.
    let targets = [
        (VideoCodec::H265, "temp_h265.mp4"),
        (VideoCodec::Vp9, "temp_vp9.webm"),
        (VideoCodec::Av1, "temp_av1.mkv"),
    ];

    for (codec, name) in targets {
        let output = format!("{DIR}{name}");

        let video = VideoConfig::builder()
            .codec(codec)
            .bitrate(Bitrate::mbps(1))
            .build()?;

        let summary = Transcoder::builder()
            .input(INPUT)
            .output(&output)
            .video(video)
            .drop_audio()
            .build()?
            .run()?;

        println!("{codec:?}: wrote {name} — {} frames, {:.2}s", summary.frames, summary.duration_secs);
    }

    Ok(())
}
