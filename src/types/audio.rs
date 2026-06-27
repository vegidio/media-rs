//! Audio sampling parameters.

/// An audio sample rate, in Hz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SampleRate {
    /// 8 kHz.
    Hz8000,
    /// 16 kHz.
    Hz16000,
    /// 22.05 kHz.
    Hz22050,
    /// 44.1 kHz (CD quality).
    Hz44100,
    /// 48 kHz (the common video-audio rate).
    Hz48000,
    /// 96 kHz.
    Hz96000,
    /// An arbitrary rate in Hz.
    Hz(u32),
}

impl SampleRate {
    /// The rate as an integer number of Hz.
    pub fn hz(self) -> i32 {
        match self {
            SampleRate::Hz8000 => 8_000,
            SampleRate::Hz16000 => 16_000,
            SampleRate::Hz22050 => 22_050,
            SampleRate::Hz44100 => 44_100,
            SampleRate::Hz48000 => 48_000,
            SampleRate::Hz96000 => 96_000,
            SampleRate::Hz(n) => n as i32,
        }
    }
}
