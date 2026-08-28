//! UI-independent workflow for calibrating a clock against minute boundaries.
//!
//! The workflow deliberately keeps wall-clock time (`SystemTime`) separate from
//! elapsed time (`Instant`). A wall-clock jump is not allowed to produce a
//! panic, an invalid duration, or a stale calibration target.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A time source used by [`Calibration`].
///
/// Keeping both clocks in a sample makes the production implementation safe to
/// use with clock adjustments while allowing deterministic tests.
pub trait Clock {
    fn sample(&self) -> ClockSample;
}

/// A simultaneous wall-clock and monotonic-clock reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockSample {
    pub wall: SystemTime,
    pub monotonic: Instant,
}

/// The normal operating-system time source.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn sample(&self) -> ClockSample {
        ClockSample {
            wall: SystemTime::now(),
            monotonic: Instant::now(),
        }
    }
}

/// The result of marking the calibration target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CalibrationResult {
    /// The minute boundary the user was expected to mark.
    pub expected: SystemTime,
    /// The wall-clock time at which the mark was made.
    pub marked: SystemTime,
    /// The absolute elapsed difference between `expected` and `marked`.
    pub difference: Duration,
    /// `true` when the mark occurred after the expected boundary.
    pub marked_after_expected: bool,
}

/// The current UI-independent state of a calibration session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CalibrationView {
    pub enabled: bool,
    /// Time remaining until the target, or zero if it has been reached.
    pub countdown: Option<Duration>,
    pub result: Option<CalibrationResult>,
}

/// A minute-boundary calibration workflow.
#[derive(Debug)]
pub struct Calibration<C = SystemClock> {
    clock: C,
    enabled: bool,
    expected: Option<SystemTime>,
    result: Option<CalibrationResult>,
    previous: Option<ClockSample>,
}

impl Calibration<SystemClock> {
    pub fn new() -> Self {
        Self::with_clock(SystemClock)
    }
}

impl Default for Calibration<SystemClock> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: Clock> Calibration<C> {
    pub fn with_clock(clock: C) -> Self {
        Self {
            clock,
            enabled: false,
            expected: None,
            result: None,
            previous: None,
        }
    }

    /// Enables a fresh session and selects the next minute boundary.
    pub fn enable(&mut self) {
        let sample = self.clock.sample();
        self.enabled = true;
        self.expected = Some(next_minute_boundary(sample.wall));
        self.result = None;
        self.previous = Some(sample);
    }

    /// Disables the session. A completed result remains available until the
    /// next call to [`Self::enable`], which makes disabling safe for a UI that
    /// hides controls without losing the displayed result.
    pub fn disable(&mut self) {
        self.enabled = false;
        self.expected = None;
        self.previous = None;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Refreshes and returns state suitable for rendering.
    pub fn view(&mut self) -> CalibrationView {
        if !self.enabled {
            return CalibrationView {
                enabled: false,
                countdown: None,
                result: self.result,
            };
        }

        let sample = self.clock.sample();
        self.recover_from_clock_change(sample);
        let countdown = self.expected.map(|expected| {
            expected
                .duration_since(sample.wall)
                .unwrap_or(Duration::ZERO)
        });
        self.previous = Some(sample);

        CalibrationView {
            enabled: true,
            countdown,
            result: self.result,
        }
    }

    /// Marks the current time against the expected minute boundary.
    ///
    /// Once marked, subsequent calls return the original result and do not
    /// replace it, so double-clicks or repeated input events are harmless.
    pub fn mark(&mut self) -> Option<CalibrationResult> {
        if !self.enabled {
            return None;
        }
        if let Some(result) = self.result {
            return Some(result);
        }

        let sample = self.clock.sample();
        self.recover_from_clock_change(sample);
        let expected = self.expected?;
        let (difference, marked_after_expected) = match sample.wall.duration_since(expected) {
            Ok(late) => (late, true),
            Err(_) => (
                expected
                    .duration_since(sample.wall)
                    .unwrap_or(Duration::ZERO),
                false,
            ),
        };
        let result = CalibrationResult {
            expected,
            marked: sample.wall,
            difference,
            marked_after_expected,
        };
        self.result = Some(result);
        self.previous = Some(sample);
        Some(result)
    }

    fn recover_from_clock_change(&mut self, current: ClockSample) {
        let Some(previous) = self.previous else {
            self.previous = Some(current);
            return;
        };

        let wall_delta = signed_duration(previous.wall, current.wall);
        let monotonic_delta = current
            .monotonic
            .checked_duration_since(previous.monotonic)
            .unwrap_or(Duration::ZERO);
        // Normal sampling differs by only the time needed to take two reads.
        // A two-second tolerance avoids treating scheduler delays as clock
        // changes while still recovering promptly from ordinary adjustments.
        if abs_difference(wall_delta, monotonic_delta) > Duration::from_secs(2) {
            self.expected = Some(next_minute_boundary(current.wall));
            self.result = None;
        }
    }
}

/// Returns the next boundary strictly after `now` on the Unix minute grid.
pub fn next_minute_boundary(now: SystemTime) -> SystemTime {
    match now.duration_since(UNIX_EPOCH) {
        Ok(age) => {
            let boundary_seconds = age.as_secs() - age.as_secs() % 60 + 60;
            UNIX_EPOCH
                .checked_add(Duration::from_secs(boundary_seconds))
                .expect("minute boundary overflow")
        }
        Err(before_epoch) => {
            let age = before_epoch.duration();
            let seconds = age.as_secs();
            let remainder = seconds % 60;
            let boundary_age = if remainder == 0 && age.subsec_nanos() == 0 {
                seconds.saturating_sub(60)
            } else {
                seconds - remainder
            };
            UNIX_EPOCH
                .checked_sub(Duration::from_secs(boundary_age))
                .unwrap_or(UNIX_EPOCH)
        }
    }
}

fn signed_duration(from: SystemTime, to: SystemTime) -> i128 {
    match to.duration_since(from) {
        Ok(duration) => duration.as_nanos() as i128,
        Err(_) => -(from.duration_since(to).unwrap_or(Duration::ZERO).as_nanos() as i128),
    }
}

fn abs_difference(left: i128, right: Duration) -> Duration {
    let difference = (left - right.as_nanos() as i128).unsigned_abs();
    Duration::from_nanos(difference.min(u64::MAX as u128) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug)]
    struct FakeClock {
        sample: ClockSample,
    }

    impl Clock for FakeClock {
        fn sample(&self) -> ClockSample {
            self.sample
        }
    }

    fn time(seconds: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(seconds)
    }

    fn set(clock: &mut FakeClock, wall_seconds: u64, monotonic_seconds: u64) {
        clock.sample = ClockSample {
            wall: time(wall_seconds),
            monotonic: Instant::now() + Duration::from_secs(monotonic_seconds),
        };
    }

    #[test]
    fn counts_down_to_the_next_boundary_and_marks_late() {
        let mut clock = FakeClock {
            sample: ClockSample {
                wall: time(65),
                monotonic: Instant::now(),
            },
        };
        let mut calibration = Calibration::with_clock(clock.clone());
        calibration.enable();
        assert_eq!(calibration.view().countdown, Some(Duration::from_secs(55)));

        set(&mut clock, 62, 3);
        // The clock is injected by value, so create the deterministic session
        // at the desired point for the mark calculation.
        let mut calibration = Calibration::with_clock(clock);
        calibration.enable();
        set(&mut calibration.clock, 120, 58);
        let result = calibration.mark().unwrap();
        assert_eq!(result.expected, time(180));
        assert_eq!(result.difference, Duration::from_secs(60));
        assert!(!result.marked_after_expected);
    }

    #[test]
    fn repeated_marks_are_idempotent() {
        let clock = FakeClock {
            sample: ClockSample {
                wall: time(119),
                monotonic: Instant::now(),
            },
        };
        let mut calibration = Calibration::with_clock(clock);
        calibration.enable();
        let first = calibration.mark().unwrap();
        let second = calibration.mark().unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn a_wall_clock_jump_restarts_the_target() {
        let mut clock = FakeClock {
            sample: ClockSample {
                wall: time(65),
                monotonic: Instant::now(),
            },
        };
        let mut calibration = Calibration::with_clock(clock.clone());
        calibration.enable();
        clock.sample = ClockSample {
            wall: time(5),
            monotonic: Instant::now() + Duration::from_secs(1),
        };
        calibration.clock = clock;
        assert_eq!(calibration.view().countdown, Some(Duration::from_secs(55)));
        assert_eq!(calibration.mark().unwrap().expected, time(60));
    }

    #[test]
    fn disabling_prevents_marks_and_reenabling_clears_result() {
        let clock = FakeClock {
            sample: ClockSample {
                wall: time(119),
                monotonic: Instant::now(),
            },
        };
        let mut calibration = Calibration::with_clock(clock);
        calibration.enable();
        assert!(calibration.mark().is_some());
        calibration.disable();
        assert!(calibration.mark().is_none());
        assert!(calibration.view().result.is_some());
        calibration.enable();
        assert!(calibration.view().result.is_none());
    }
}
