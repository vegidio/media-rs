//! Audio-only transcoding with the one-liner API: convert formats and set a bitrate without
//! ever leaving the chained call.
//!
//! Shows: `transcode(input)` on an audio-only file, format conversion (MP3 → FLAC), and
//! re-encoding via `.audio(AudioConfig::builder()…)`.
//!
//! Run with: `cargo run --example transcode_audio`

use media::prelude::*;

const INPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/audio1.mp3");
const FLAC_OUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/temp_audio.flac");
const AAC_OUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/temp_audio.m4a");

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Lossless format conversion — the container (.flac) picks the codec; nothing else to set.
    transcode(INPUT).to(FLAC_OUT).run()?;
    println!("Wrote {FLAC_OUT}");

    // Re-encode to AAC at a chosen bitrate. Setting the codec is optional here (the .m4a
    // container already defaults to AAC) but makes the intent explicit.
    let summary = transcode(INPUT)
        .to(AAC_OUT)
        .audio(
            AudioConfig::builder()
                .codec(AudioCodec::Aac)
                .bitrate(Bitrate::kbps(128))
                .sample_rate(SampleRate::Hz44100)
                .build()?,
        )
        .run()?;

    println!("Wrote {AAC_OUT} ({:.2}s)", summary.duration_secs);
    Ok(())
}
