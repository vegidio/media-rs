//! Encoder speed/quality presets.

/// An x264/x265 speed↔compression preset. Slower presets produce smaller files at the same
/// quality. Maps to the encoder's `preset` private option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum H264Preset {
    /// Fastest, largest output.
    Ultrafast,
    /// Very fast.
    Superfast,
    /// Faster than `Fast`.
    Veryfast,
    /// Faster.
    Faster,
    /// Fast.
    Fast,
    /// The encoder default.
    Medium,
    /// Slow.
    Slow,
    /// Slower.
    Slower,
    /// Slowest, smallest output.
    Veryslow,
}

impl H264Preset {
    /// The string value for the encoder's `preset` option.
    pub fn as_str(self) -> &'static str {
        match self {
            H264Preset::Ultrafast => "ultrafast",
            H264Preset::Superfast => "superfast",
            H264Preset::Veryfast => "veryfast",
            H264Preset::Faster => "faster",
            H264Preset::Fast => "fast",
            H264Preset::Medium => "medium",
            H264Preset::Slow => "slow",
            H264Preset::Slower => "slower",
            H264Preset::Veryslow => "veryslow",
        }
    }
}
