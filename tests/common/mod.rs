//! Shared helpers for integration tests.

use std::path::{Path, PathBuf};

/// Absolute path to an asset file under `assets/`.
pub fn asset(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join(name)
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
