//! Encoder profiles.

/// An H.264 profile, constraining the feature set so the output decodes on a target tier of
/// hardware. Maps to the encoder's `profile` private option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum H264Profile {
    /// Baseline — widest device compatibility, fewest tools.
    Baseline,
    /// Main.
    Main,
    /// High — best compression, the common default for SD/HD.
    High,
}

impl H264Profile {
    /// The string value for the encoder's `profile` option.
    pub fn as_str(self) -> &'static str {
        match self {
            H264Profile::Baseline => "baseline",
            H264Profile::Main => "main",
            H264Profile::High => "high",
        }
    }
}
