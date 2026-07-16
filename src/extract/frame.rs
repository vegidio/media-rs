//! A single extracted video frame, with its pixels ready for encoding or inspection.

use crate::error::{Error, Result};
use crate::extract::types::ImageFormat;
use image::{DynamicImage, RgbImage};
use std::path::Path;
use std::time::Duration;

/// One frame produced by the extraction API.
///
/// Carries the frame's packed RGB pixels plus where it sits in the run ([`index`](Self::index))
/// and when it occurs in the source ([`timestamp`](Self::timestamp)). Encode it to bytes with
/// [`encode`](Self::encode), write it to a file with [`save`](Self::save), or read the raw
/// pixels with [`to_rgb_bytes`](Self::to_rgb_bytes).
pub struct ExtractedFrame {
    index: u32,
    timestamp: Duration,
    width: u32,
    height: u32,
    rgb: Vec<u8>,
}

impl ExtractedFrame {
    pub(crate) fn new(index: u32, timestamp: Duration, width: u32, height: u32, rgb: Vec<u8>) -> Self {
        Self {
            index,
            timestamp,
            width,
            height,
            rgb,
        }
    }

    /// This frame's zero-based position in the extraction run.
    pub fn index(&self) -> u32 {
        self.index
    }

    /// When this frame occurs in the source video.
    pub fn timestamp(&self) -> Duration {
        self.timestamp
    }

    /// The frame's `(width, height)` in pixels.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The raw, tightly packed RGB (8:8:8) pixels: row-major, `width * height * 3` bytes.
    pub fn to_rgb_bytes(&self) -> &[u8] {
        &self.rgb
    }

    /// Encode this frame to image bytes in `format`.
    pub fn encode(&self, format: ImageFormat) -> Result<Vec<u8>> {
        let img = self.dynamic_image()?;
        let mut bytes = Vec::new();
        rust_sak::image::encode_writer(&img, &mut bytes, format.rsak_format(), Some(format.rsak_options()))
            .map_err(|e| Error::ImageEncode(e.to_string()))?;
        Ok(bytes)
    }

    /// Save this frame to `path`, choosing the format from the file extension
    /// (`.jpg`/`.jpeg`/`.png`, …). Delegates the whole encode-and-write to `rust-sak`.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let img = self.dynamic_image()?;
        rust_sak::image::encode_file(&img, path, None)
            .map_err(|e| Error::ImageEncode(format!("writing {}: {e}", path.display())))
    }

    /// Save to `path` with an explicit format's encode settings (quality/compression). The
    /// path's extension must match `format`; used by the directory output, which names files
    /// with `format`'s extension.
    pub(crate) fn save_as(&self, path: impl AsRef<Path>, format: ImageFormat) -> Result<()> {
        let path = path.as_ref();
        let img = self.dynamic_image()?;
        rust_sak::image::encode_file(&img, path, Some(format.rsak_options()))
            .map_err(|e| Error::ImageEncode(format!("writing {}: {e}", path.display())))
    }

    /// Build an `image::DynamicImage` view over the packed RGB pixels (the only step that
    /// touches the `image` crate directly, as `rust-sak`'s encoders take a `DynamicImage`).
    fn dynamic_image(&self) -> Result<DynamicImage> {
        RgbImage::from_raw(self.width, self.height, self.rgb.clone())
            .map(DynamicImage::ImageRgb8)
            .ok_or_else(|| Error::ImageEncode("RGB buffer does not match frame dimensions".to_owned()))
    }
}
