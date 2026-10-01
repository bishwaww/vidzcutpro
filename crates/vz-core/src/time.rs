//! Time in vidzcut is a whole number of microseconds ([`Micros`]). Never a decimal.
//!
//! Frame rates are exact fractions (29.97 is 30000/1001), so a frame never drifts
//! away from where it belongs, however long the project is.

use serde::{Deserialize, Serialize};

/// A point in time or a length of time, in microseconds.
pub type Micros = i64;

/// Microseconds in one second.
pub const US_PER_SEC: Micros = 1_000_000;

/// A frame rate as an exact fraction: `num / den` frames per second.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FrameRate {
    pub num: u32,
    pub den: u32,
}

impl FrameRate {
    pub const FPS_23_976: Self = Self {
        num: 24000,
        den: 1001,
    };
    pub const FPS_24: Self = Self { num: 24, den: 1 };
    pub const FPS_25: Self = Self { num: 25, den: 1 };
    pub const FPS_29_97: Self = Self {
        num: 30000,
        den: 1001,
    };
    pub const FPS_30: Self = Self { num: 30, den: 1 };
    pub const FPS_50: Self = Self { num: 50, den: 1 };
    pub const FPS_59_94: Self = Self {
        num: 60000,
        den: 1001,
    };
    pub const FPS_60: Self = Self { num: 60, den: 1 };

    /// The frame rates offered in the "New project" dialog.
    pub const PRESETS: [Self; 8] = [
        Self::FPS_23_976,
        Self::FPS_24,
        Self::FPS_25,
        Self::FPS_29_97,
        Self::FPS_30,
        Self::FPS_50,
        Self::FPS_59_94,
        Self::FPS_60,
    ];

    /// Make a frame rate. Returns `None` if either part is zero.
    pub fn new(num: u32, den: u32) -> Option<Self> {
        (num > 0 && den > 0).then_some(Self { num, den })
    }

    /// For showing to people only. Never use this for editing maths.
    pub fn as_f64(self) -> f64 {
        f64::from(self.num) / f64::from(self.den)
    }

    /// Whole frames per second used when writing timecodes (29.97 counts as 30).
    pub fn nominal(self) -> i64 {
        (i64::from(self.num) + i64::from(self.den) / 2) / i64::from(self.den)
    }

    /// Text such as `29.97` or `30`.
    pub fn label(self) -> String {
        if self.den == 1 {
            format!("{}", self.num)
        } else {
            format!("{:.3}", self.as_f64())
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string()
        }
    }

    /// Where frame number `n` starts, rounded to the nearest microsecond.
    pub fn frame_start(self, n: i64) -> Micros {
        let num = i128::from(self.num);
        let top = i128::from(n) * i128::from(US_PER_SEC) * i128::from(self.den);
        // round half up, also correct for negative n
        ((2 * top + num).div_euclid(2 * num)) as Micros
    }

    /// The frame that is showing at time `t` (the last frame that started at or before `t`).
    pub fn frame_at(self, t: Micros) -> i64 {
        let guess = (i128::from(t) * i128::from(self.num))
            .div_euclid(i128::from(US_PER_SEC) * i128::from(self.den)) as i64;
        let mut n = guess - 1;
        while self.frame_start(n + 1) <= t {
            n += 1;
        }
        n
    }

    /// Snap a time to the nearest frame boundary.
    pub fn snap(self, t: Micros) -> Micros {
        let n = self.frame_at(t);
        let a = self.frame_start(n);
        let b = self.frame_start(n + 1);
        if t - a <= b - t { a } else { b }
    }

    /// `HH:MM:SS:FF` timecode (non-drop-frame) for time `t`.
    pub fn timecode(self, t: Micros) -> String {
        let sign = if t < 0 { "-" } else { "" };
        let n = self.frame_at(t.abs());
        let fps = self.nominal().max(1);
        let (ff, secs) = (n % fps, n / fps);
        format!(
            "{sign}{:02}:{:02}:{:02}:{:02}",
            secs / 3600,
            (secs / 60) % 60,
            secs % 60,
            ff
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_rates_are_exact() {
        assert_eq!(FrameRate::FPS_30.frame_start(30), US_PER_SEC);
        assert_eq!(FrameRate::FPS_25.frame_start(1), 40_000);
    }

    #[test]
    fn ntsc_rates_do_not_drift() {
        // 24 frames at 23.976 is exactly 1.001 s; 30000 frames at 29.97 is exactly 1001 s.
        assert_eq!(FrameRate::FPS_23_976.frame_start(24), 1_001_000);
        assert_eq!(FrameRate::FPS_29_97.frame_start(30_000), 1_001_000_000);
        // a 10-hour project is still exact
        assert_eq!(
            FrameRate::FPS_29_97.frame_start(30_000 * 36),
            36_036_000_000
        );
    }

    #[test]
    fn frame_at_is_inverse_of_frame_start() {
        for rate in FrameRate::PRESETS {
            for n in [0, 1, 2, 23, 24, 29, 30, 1001, 86_399, 1_000_000] {
                let start = rate.frame_start(n);
                assert_eq!(rate.frame_at(start), n, "{rate:?} frame {n}");
                assert_eq!(
                    rate.frame_at(start - 1),
                    n - 1,
                    "{rate:?} just before frame {n}"
                );
            }
        }
    }

    #[test]
    fn snap_goes_to_nearest_frame() {
        let r = FrameRate::FPS_25; // frames every 40 000 us
        assert_eq!(r.snap(19_999), 0);
        assert_eq!(r.snap(20_000), 0);
        assert_eq!(r.snap(20_001), 40_000);
        assert_eq!(r.snap(40_000), 40_000);
    }

    #[test]
    fn timecode_text() {
        assert_eq!(FrameRate::FPS_30.timecode(0), "00:00:00:00");
        assert_eq!(
            FrameRate::FPS_30.timecode(61 * US_PER_SEC + 33_334),
            "00:01:01:01"
        );
        assert_eq!(
            FrameRate::FPS_25.timecode(3_600 * US_PER_SEC),
            "01:00:00:00"
        );
    }

    #[test]
    fn new_rejects_zero() {
        assert!(FrameRate::new(0, 1).is_none());
        assert!(FrameRate::new(30, 0).is_none());
        assert_eq!(FrameRate::new(30, 1), Some(FrameRate::FPS_30));
    }

    #[test]
    fn labels() {
        assert_eq!(FrameRate::FPS_30.label(), "30");
        assert_eq!(FrameRate::FPS_29_97.label(), "29.97");
        assert_eq!(FrameRate::FPS_23_976.label(), "23.976");
    }
}
