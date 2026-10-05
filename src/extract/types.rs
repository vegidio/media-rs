//! Typed configuration for frame extraction: sampling interval, image format, output
//! resolution, file naming, and where extracted frames go.

use crate::error::Result;
use crate::extract::frame::ExtractedFrame;
use rust_sak::image::{EncodeOptions, ImageFormat as RsakFormat, PngCompression, PngFilter};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Quality used for JPEG when a caller doesn't specify one (via `Default` or extension
/// inference). Kept in one place so the two entry points can't drift apart.
const DEFAULT_JPEG_QUALITY: u8 = 90;

/// How to choose which frames to extract.
#[derive(Debug, Clone, PartialEq)]
pub enum Interval {
    /// One frame every N seconds (e.g. `EverySeconds(1.0)` → 0s, 1s, 2s, …).
    EverySeconds(f64),
    /// Every N-th decoded frame (e.g. `EveryNFrames(30)` → frames 0, 30, 60, …).
    EveryNFrames(u32),
    /// Sample at this many frames per second regardless of the source rate. Equivalent to
    /// `EverySeconds(1.0 / fps)`.
    Fps(f64),
    /// Exactly this many frames, evenly spaced across the (trimmed) duration.
    Count(u32),
    /// Frames at these exact timestamps.
    Timestamps(Vec<Duration>),
}

/// The image format (and quality) extracted frames are encoded to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    /// JPEG at the given quality (1–100).
    Jpeg {
        /// Encoder quality: 1 (smallest) – 100 (best).
        quality: u8,
    },
    /// Lossless PNG.
    Png,
}

impl Default for ImageFormat {
    fn default() -> Self {
        ImageFormat::Jpeg { quality: DEFAULT_JPEG_QUALITY }
    }
}

impl ImageFormat {
    /// The conventional file extension for this format (without a leading dot).
    pub fn extension(self) -> &'static str {
        match self {
            ImageFormat::Jpeg { .. } => "jpg",
            ImageFormat::Png => "png",
        }
    }

    /// Infer a format from a file-name extension (`jpg`/`jpeg` → JPEG, `png` → PNG). JPEG
    /// defaults to quality 90.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "jpg" | "jpeg" => Some(ImageFormat::Jpeg { quality: DEFAULT_JPEG_QUALITY }),
            "png" => Some(ImageFormat::Png),
            _ => None,
        }
    }

    /// Infer a format from a path's extension.
    pub fn from_path(path: impl AsRef<Path>) -> Option<Self> {
        path.as_ref().extension().and_then(|e| e.to_str()).and_then(Self::from_extension)
    }

    /// The matching `rust-sak` container format.
    pub(crate) fn rsak_format(self) -> RsakFormat {
        match self {
            ImageFormat::Jpeg { .. } => RsakFormat::Jpeg,
            ImageFormat::Png => RsakFormat::Png,
        }
    }

    /// The matching `rust-sak` encode options (carrying quality/compression settings).
    pub(crate) fn rsak_options(self) -> EncodeOptions {
        match self {
            ImageFormat::Jpeg { quality } => EncodeOptions::Jpeg { quality },
            ImageFormat::Png => {
                EncodeOptions::Png { compression: PngCompression::default(), filter: PngFilter::default() }
            }
        }
    }
}

/// The output resolution of extracted frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Resolution {
    /// Keep the source frame's dimensions.
    #[default]
    Original,
    /// Scale every frame to exactly `width`×`height`.
    Fixed(u32, u32),
    /// Scale every frame so its longer edge is `bound` pixels, keeping the aspect ratio. A frame
    /// that already fits is passed through at its own size — never enlarged. The bound must be at
    /// least 1.
    Fit(u32),
}

impl Resolution {
    /// The filter chain that converts a decoded `width`×`height` frame to packed RGB24 at this
    /// resolution.
    pub(crate) fn filter_description(self, width: u32, height: u32) -> String {
        match self {
            Resolution::Original => "format=rgb24".to_owned(),
            Resolution::Fixed(w, h) => format!("scale={w}:{h},format=rgb24"),
            Resolution::Fit(bound) => match fit_dimensions(width, height, bound) {
                Some((w, h)) => format!("scale={w}:{h}:flags=lanczos,format=rgb24"),
                None => "format=rgb24".to_owned(),
            },
        }
    }
}

/// The size of a `width`×`height` frame scaled so its longer edge is `bound`, or `None` when it
/// already fits. The short edge is rounded to the nearest pixel and is at least 1.
fn fit_dimensions(width: u32, height: u32, bound: u32) -> Option<(u32, u32)> {
    let longer = width.max(height);
    if longer <= bound {
        return None;
    }

    // In `u64` so the product can't overflow; the result is at most `bound`, so it fits a `u32`.
    let short_edge =
        |short: u32| ((u64::from(short) * u64::from(bound) + u64::from(longer) / 2) / u64::from(longer)) as u32;

    Some(if width >= height {
        (bound, short_edge(height).max(1))
    } else {
        (short_edge(width).max(1), bound)
    })
}

/// How output files are named within the destination directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NamingScheme {
    /// `<prefix>_<index>.<ext>`, zero-padded to `padding` digits (e.g. `frame_0000.jpg`).
    Sequential {
        /// Filename stem before the index.
        prefix: String,
        /// Zero-padding width for the index.
        padding: usize,
    },
}

impl Default for NamingScheme {
    fn default() -> Self {
        NamingScheme::Sequential { prefix: "frame".to_owned(), padding: 4 }
    }
}

impl NamingScheme {
    /// The file name for the frame at `index`, using extension `ext`.
    pub(crate) fn file_name(&self, index: u32, ext: &str) -> String {
        match self {
            NamingScheme::Sequential { prefix, padding } => {
                format!("{prefix}_{index:0width$}.{ext}", width = *padding)
            }
        }
    }
}

/// Where extracted frames are delivered.
pub enum Output {
    /// Write each frame as an image file into this directory (created if missing).
    Directory(PathBuf),
    /// Keep every frame in memory; retrieve them from
    /// [`ExtractReport::frames`](crate::extract::ExtractReport::frames).
    InMemory,
    /// Hand each frame to a callback as it is produced (nothing is buffered).
    Callback(Box<dyn FnMut(ExtractedFrame) -> Result<()>>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequential_naming_zero_pads_the_index() {
        let scheme = NamingScheme::default();
        assert_eq!(scheme.file_name(0, "jpg"), "frame_0000.jpg");
        assert_eq!(scheme.file_name(42, "png"), "frame_0042.png");
        let wide = NamingScheme::Sequential { prefix: "shot".to_owned(), padding: 2 };
        assert_eq!(wide.file_name(7, "jpg"), "shot_07.jpg");
        // An index exceeding the padding width is not truncated.
        assert_eq!(wide.file_name(1234, "jpg"), "shot_1234.jpg");
    }

    #[test]
    fn image_format_extension_and_inference() {
        assert_eq!(ImageFormat::Jpeg { quality: 80 }.extension(), "jpg");
        assert_eq!(ImageFormat::Png.extension(), "png");
        assert_eq!(ImageFormat::from_extension("JPEG"), Some(ImageFormat::Jpeg { quality: 90 }));
        assert_eq!(ImageFormat::from_path("a/b/c.png"), Some(ImageFormat::Png));
        assert_eq!(ImageFormat::from_extension("gif"), None);
    }

    #[test]
    fn fit_scales_the_longer_edge_to_the_bound() {
        assert_eq!(fit_dimensions(1920, 1080, 320), Some((320, 180)));
        assert_eq!(fit_dimensions(1080, 1920, 320), Some((180, 320)));
        assert_eq!(fit_dimensions(1000, 1000, 256), Some((256, 256)));
        // 676 * 100 / 1280 = 52.8, rounded to the nearest pixel.
        assert_eq!(fit_dimensions(676, 1280, 100), Some((53, 100)));
    }

    #[test]
    fn fit_never_enlarges() {
        assert_eq!(fit_dimensions(1280, 720, 1280), None);
        assert_eq!(fit_dimensions(1280, 720, 4096), None);
    }

    #[test]
    fn fit_keeps_a_short_edge_of_at_least_one() {
        assert_eq!(fit_dimensions(10_000, 1, 100), Some((100, 1)));
        assert_eq!(fit_dimensions(3, 10_000, 16), Some((1, 16)));
    }

    #[test]
    fn filter_description_per_resolution() {
        assert_eq!(Resolution::Original.filter_description(1280, 720), "format=rgb24");
        assert_eq!(Resolution::Fixed(64, 36).filter_description(1280, 720), "scale=64:36,format=rgb24");
        assert_eq!(Resolution::Fit(320).filter_description(1280, 720), "scale=320:180:flags=lanczos,format=rgb24");
        assert_eq!(Resolution::Fit(2000).filter_description(1280, 720), "format=rgb24");
    }
}
