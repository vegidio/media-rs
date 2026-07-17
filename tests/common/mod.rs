//! Shared helpers for integration tests.
//!
//! Each test binary `mod`-includes this file, so a helper unused by one binary would warn
//! there; allow dead code for the module as a whole.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// Absolute path to an asset file under `assets/`.
pub fn asset(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join(name)
}

/// A path to `name` in the system temp directory, as a string.
pub fn temp(name: &str) -> String {
    std::env::temp_dir().join(name).to_str().unwrap().to_owned()
}

/// The sample videos, skipped gracefully if the `assets/` folder isn't present (it is
/// excluded from the published crate).
pub fn sample_videos() -> Vec<PathBuf> {
    ["video1.mp4", "video2.mp4", "video3.mp4"]
        .iter()
        .map(|n| asset(n))
        .filter(|p| p.exists())
        .collect()
}

/// The one sample that carries an audio stream (`video2.mp4`), or `None` when assets are
/// absent. Audio-path tests use this so they exercise real demux/decode/stream-copy.
pub fn audio_sample() -> Option<PathBuf> {
    let p = asset("video2.mp4");
    p.exists().then_some(p)
}

/// The audio-only sample (`audio1.mp3`), or `None` when assets are absent. Used by tests that
/// exercise the audio-only paths (format conversion, auto-encode) which the video-with-audio
/// sample can't cover as cleanly.
pub fn audio_only_sample() -> Option<PathBuf> {
    let p = asset("audio1.mp3");
    p.exists().then_some(p)
}
