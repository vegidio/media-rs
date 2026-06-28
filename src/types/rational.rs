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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rational_as_f64_handles_zero_denominator() {
        assert_eq!(Rational::new(1, 2).as_f64(), 0.5);
        assert_eq!(Rational::new(30000, 1001).as_f64(), 30000.0 / 1001.0);
        // A zero denominator must not divide-by-zero; it reports 0.0.
        assert_eq!(Rational::new(5, 0).as_f64(), 0.0);
    }

    #[test]
    fn rational_roundtrips_through_av() {
        let r = Rational::new(24000, 1001);
        let av: sys::AVRational = r.into();
        assert_eq!(av.num, 24000);
        assert_eq!(av.den, 1001);
        assert_eq!(Rational::from(av), r);
        // The pub(crate) helpers agree with the From impls.
        assert_eq!(Rational::from_av(r.to_av()), r);
    }

    #[test]
    fn framerate_constructors_and_time_base() {
        assert_eq!(Framerate::fps(30), Framerate(Rational::new(30, 1)));
        assert_eq!(Framerate::ratio(30000, 1001).as_f64(), 30000.0 / 1001.0);
        // The time base is the reciprocal of the frame rate.
        assert_eq!(Framerate::fps(25).time_base(), Rational::new(1, 25));
    }

    #[test]
    fn bitrate_unit_conversions() {
        assert_eq!(Bitrate::bps(800).as_bps(), 800);
        assert_eq!(Bitrate::kbps(128).as_bps(), 128_000);
        assert_eq!(Bitrate::mbps(5).as_bps(), 5_000_000);
        // Derived Ord compares by the underlying bps.
        assert!(Bitrate::kbps(128) < Bitrate::mbps(1));
    }
}
