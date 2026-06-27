//! Numeric newtypes: rationals, frame rates, and bit rates.

use crate::sys;

/// A rational number, mirroring [`sys::AVRational`] (`num / den`).
///
/// FFmpeg uses rationals pervasively for time bases and frame rates so that exact values
/// like `1/30000` survive arithmetic without floating-point drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rational {
    /// Numerator.
    pub num: i32,
    /// Denominator.
    pub den: i32,
}

impl Rational {
    /// Create a rational `num / den`.
    pub const fn new(num: i32, den: i32) -> Self {
        Self { num, den }
    }

    /// Evaluate as an `f64` (`num / den`). Returns `0.0` for a zero denominator.
    pub fn as_f64(self) -> f64 {
        if self.den == 0 {
            0.0
        } else {
            self.num as f64 / self.den as f64
        }
    }

    pub(crate) fn to_av(self) -> sys::AVRational {
        sys::AVRational {
            num: self.num,
            den: self.den,
        }
    }

    pub(crate) fn from_av(r: sys::AVRational) -> Self {
        Self {
            num: r.num,
            den: r.den,
        }
    }
}

impl From<sys::AVRational> for Rational {
    fn from(r: sys::AVRational) -> Self {
        Rational::from_av(r)
    }
}

impl From<Rational> for sys::AVRational {
    fn from(r: Rational) -> Self {
        r.to_av()
    }
}

/// A frame rate, in frames per second, stored as an exact rational.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Framerate(pub Rational);

impl Framerate {
    /// An integer frame rate, e.g. `Framerate::fps(30)` → `30/1`.
    pub const fn fps(fps: u32) -> Self {
        Self(Rational::new(fps as i32, 1))
    }

    /// A rational frame rate, e.g. `Framerate::ratio(30000, 1001)` for 29.97 fps.
    pub const fn ratio(num: i32, den: i32) -> Self {
        Self(Rational::new(num, den))
    }

    /// The frame rate as a floating-point value.
    pub fn as_f64(self) -> f64 {
        self.0.as_f64()
    }

    /// The matching codec time base (the reciprocal of the frame rate).
    pub(crate) fn time_base(self) -> Rational {
        Rational::new(self.0.den, self.0.num)
    }
}

/// A bit rate in bits per second.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Bitrate(pub i64);

impl Bitrate {
    /// From bits per second.
    pub const fn bps(bps: i64) -> Self {
        Self(bps)
    }

    /// From kilobits per second (×1000).
    pub const fn kbps(kbps: i64) -> Self {
        Self(kbps * 1_000)
    }

    /// From megabits per second (×1_000_000).
    pub const fn mbps(mbps: i64) -> Self {
        Self(mbps * 1_000_000)
    }

    /// The value in bits per second.
    pub const fn as_bps(self) -> i64 {
        self.0
    }
}
