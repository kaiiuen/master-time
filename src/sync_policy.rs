//! UI-independent policy for deciding whether an NTP measurement may be used.
//!
//! This module only classifies measurements. It does not apply corrections, read
//! the system clock, or invoke platform commands.

use crate::health::{self, HealthStatus};
use crate::measurement::Measurement;

/// The default absolute offset at or below which a synchronized measurement may
/// be considered for correction, in seconds.
pub const DEFAULT_MAX_CORRECTION_OFFSET: f64 = 1.0;

/// The action permitted for a measurement by the synchronization policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncDisposition {
    /// The sample may be shown, but must not be used to correct the clock.
    DisplayOnly,
    /// The sample passed all checks required before a caller may offer a
    /// correction. This variant does not perform the correction itself.
    EligibleForCorrection,
    /// The sample is not safe to display as a usable synchronization result or
    /// to use for correction.
    Unsafe,
}

/// Configuration for the synchronization classification policy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SyncPolicy {
    /// Maximum permitted absolute offset for correction, in seconds.
    max_correction_offset: f64,
}

impl Default for SyncPolicy {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_CORRECTION_OFFSET)
    }
}

impl SyncPolicy {
    /// Creates a policy with an absolute correction-offset threshold in seconds.
    ///
    /// A negative or non-finite threshold is retained but makes every
    /// classification unsafe, rather than silently weakening the policy.
    pub const fn new(max_correction_offset: f64) -> Self {
        Self {
            max_correction_offset,
        }
    }

    pub const fn max_correction_offset(self) -> f64 {
        self.max_correction_offset
    }

    /// Classifies one measurement without changing any clock state.
    ///
    /// `health` should be the result of [`crate::health::evaluate`] for the
    /// response that produced `measurement`, and `stratum` is retained here as
    /// a defense-in-depth check before permitting correction. Missing or
    /// malformed measurements are always unsafe.
    pub fn classify(
        self,
        measurement: Option<Measurement>,
        health: HealthStatus,
        stratum: u8,
    ) -> SyncDisposition {
        let Some(measurement) = measurement else {
            return SyncDisposition::Unsafe;
        };

        if !self.max_correction_offset.is_finite()
            || self.max_correction_offset < 0.0
            || !measurement.offset.is_finite()
            || !measurement.root_distance.is_finite()
            || measurement.root_distance < 0.0
            || !(health::MIN_VALID_STRATUM..=health::MAX_VALID_STRATUM).contains(&stratum)
            || measurement.root_distance > health::UNCERTAIN_ROOT_DISTANCE
        {
            return SyncDisposition::Unsafe;
        }

        match health {
            HealthStatus::Unavailable => SyncDisposition::Unsafe,
            HealthStatus::Uncertain => SyncDisposition::DisplayOnly,
            HealthStatus::Synchronized => {
                if stratum > health::SYNCHRONIZED_MAX_STRATUM
                    || measurement.root_distance > health::SYNCHRONIZED_ROOT_DISTANCE
                {
                    return SyncDisposition::Unsafe;
                }

                if measurement.offset.abs() <= self.max_correction_offset {
                    SyncDisposition::EligibleForCorrection
                } else {
                    SyncDisposition::Unsafe
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::{HealthInput, LeapIndicator, evaluate};

    fn measurement(offset: f64, root_distance: f64) -> Measurement {
        Measurement {
            offset,
            round_trip_delay: 0.02,
            root_distance,
        }
    }

    fn health(available: bool, stratum: u8, root_distance: f64) -> HealthStatus {
        evaluate(HealthInput::new(
            available,
            stratum,
            LeapIndicator::NoWarning,
            root_distance,
        ))
    }

    #[test]
    fn safe_measurement_is_eligible_for_correction() {
        let policy = SyncPolicy::new(0.5);

        assert_eq!(
            policy.classify(Some(measurement(0.25, 0.25)), health(true, 2, 0.25), 2,),
            SyncDisposition::EligibleForCorrection
        );
    }

    #[test]
    fn uncertain_measurement_is_display_only() {
        let policy = SyncPolicy::new(0.5);

        assert_eq!(
            policy.classify(Some(measurement(0.25, 2.0)), health(true, 2, 2.0), 2,),
            SyncDisposition::DisplayOnly
        );
    }

    #[test]
    fn unavailable_measurement_is_unsafe() {
        let policy = SyncPolicy::default();

        assert_eq!(
            policy.classify(None, health(false, 2, 0.0), 2),
            SyncDisposition::Unsafe
        );
    }

    #[test]
    fn excessive_offset_is_unsafe() {
        let policy = SyncPolicy::new(0.5);

        assert_eq!(
            policy.classify(Some(measurement(0.500_001, 0.25)), health(true, 2, 0.25), 2,),
            SyncDisposition::Unsafe
        );
    }

    #[test]
    fn leap_alarm_is_unsafe() {
        let policy = SyncPolicy::default();
        let alarm_health = evaluate(HealthInput::new(true, 2, LeapIndicator::Alarm, 0.25));

        assert_eq!(
            policy.classify(Some(measurement(0.1, 0.25)), alarm_health, 2),
            SyncDisposition::Unsafe
        );
    }
}
