//! Standalone health evaluation for an NTP time measurement.
//!
//! The evaluator deliberately does not depend on the packet or measurement
//! modules. Callers can map their existing response data into [`HealthInput`]
//! and integrate this module without taking on any additional dependencies.

/// The greatest root distance considered synchronized, in seconds.
pub const SYNCHRONIZED_ROOT_DISTANCE: f64 = 1.0;

/// The greatest root distance considered usable but uncertain, in seconds.
pub const UNCERTAIN_ROOT_DISTANCE: f64 = 5.0;

/// Strata through this value are eligible for a synchronized result.
///
/// Stratum 1 is directly attached to a reference clock; strata 2 through 4
/// are still a reasonably short, normal synchronization chain. Higher valid
/// strata remain usable, but are reported as uncertain.
pub const SYNCHRONIZED_MAX_STRATUM: u8 = 4;

/// Valid NTP server strata. Stratum zero is a kiss-of-death/reference code,
/// not a synchronized server, and values above 15 are reserved/invalid.
pub const MIN_VALID_STRATUM: u8 = 1;
pub const MAX_VALID_STRATUM: u8 = 15;

/// NTP leap-indicator values carried by a response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeapIndicator {
    /// No leap-second warning is pending.
    NoWarning,
    /// The last minute of the current UTC day has an inserted second.
    InsertSecond,
    /// The last minute of the current UTC day has a deleted second.
    DeleteSecond,
    /// The clock is unsynchronized or the leap state is unknown.
    Alarm,
}

impl LeapIndicator {
    /// Converts the two-bit NTP leap indicator to its typed representation.
    /// Values outside the two-bit encoding are rejected.
    pub const fn from_bits(bits: u8) -> Option<Self> {
        match bits {
            0 => Some(Self::NoWarning),
            1 => Some(Self::InsertSecond),
            2 => Some(Self::DeleteSecond),
            3 => Some(Self::Alarm),
            _ => None,
        }
    }
}

/// The inputs required to evaluate one time measurement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HealthInput {
    /// Whether all data needed to produce the measurement was present.
    pub measurement_available: bool,
    /// The NTP server stratum from the response header.
    pub stratum: u8,
    /// The NTP leap indicator from the response header.
    pub leap_indicator: LeapIndicator,
    /// NTP root distance, in seconds (root delay / 2 + root dispersion).
    pub root_distance: f64,
}

impl HealthInput {
    pub const fn new(
        measurement_available: bool,
        stratum: u8,
        leap_indicator: LeapIndicator,
        root_distance: f64,
    ) -> Self {
        Self {
            measurement_available,
            stratum,
            leap_indicator,
            root_distance,
        }
    }
}

/// Coarse health state of a time measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    /// The measurement is available and meets all synchronization thresholds.
    Synchronized,
    /// The measurement is usable, but its quality warrants caution.
    Uncertain,
    /// The measurement must not be used to establish clock health.
    Unavailable,
}

impl HealthStatus {
    pub const fn is_available(self) -> bool {
        !matches!(self, Self::Unavailable)
    }

    pub const fn is_synchronized(self) -> bool {
        matches!(self, Self::Synchronized)
    }
}

/// Evaluates measurement availability and NTP synchronization quality.
///
/// Evaluation has a conservative precedence: missing data, an invalid stratum,
/// an alarm leap indicator, or a non-finite/too-large root distance makes the
/// result [`HealthStatus::Unavailable`]. A valid measurement is synchronized
/// only when it has no leap warning, a stratum no greater than
/// [`SYNCHRONIZED_MAX_STRATUM`], and root distance no greater than
/// [`SYNCHRONIZED_ROOT_DISTANCE`]. Other valid measurements within the
/// uncertain root-distance limit are [`HealthStatus::Uncertain`].
pub fn evaluate(input: HealthInput) -> HealthStatus {
    if !input.measurement_available
        || !(MIN_VALID_STRATUM..=MAX_VALID_STRATUM).contains(&input.stratum)
        || input.leap_indicator == LeapIndicator::Alarm
        || !input.root_distance.is_finite()
        || input.root_distance < 0.0
        || input.root_distance > UNCERTAIN_ROOT_DISTANCE
    {
        return HealthStatus::Unavailable;
    }

    if input.leap_indicator == LeapIndicator::NoWarning
        && input.stratum <= SYNCHRONIZED_MAX_STRATUM
        && input.root_distance <= SYNCHRONIZED_ROOT_DISTANCE
    {
        HealthStatus::Synchronized
    } else {
        HealthStatus::Uncertain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> HealthInput {
        HealthInput::new(true, 2, LeapIndicator::NoWarning, 0.25)
    }

    #[test]
    fn healthy_measurement_is_synchronized() {
        assert_eq!(evaluate(input()), HealthStatus::Synchronized);
    }

    #[test]
    fn synchronized_thresholds_are_inclusive() {
        assert_eq!(
            evaluate(HealthInput::new(
                true,
                SYNCHRONIZED_MAX_STRATUM,
                LeapIndicator::NoWarning,
                SYNCHRONIZED_ROOT_DISTANCE,
            )),
            HealthStatus::Synchronized
        );
    }

    #[test]
    fn missing_measurement_is_unavailable() {
        assert_eq!(
            evaluate(HealthInput::new(false, 2, LeapIndicator::NoWarning, 0.0)),
            HealthStatus::Unavailable
        );
    }

    #[test]
    fn stratum_zero_and_reserved_strata_are_unavailable() {
        for stratum in [0, MAX_VALID_STRATUM + 1] {
            assert_eq!(
                evaluate(HealthInput::new(
                    true,
                    stratum,
                    LeapIndicator::NoWarning,
                    0.0,
                )),
                HealthStatus::Unavailable
            );
        }
    }

    #[test]
    fn higher_valid_stratum_is_uncertain() {
        assert_eq!(
            evaluate(HealthInput::new(
                true,
                SYNCHRONIZED_MAX_STRATUM + 1,
                LeapIndicator::NoWarning,
                0.0,
            )),
            HealthStatus::Uncertain
        );
    }

    #[test]
    fn leap_warnings_make_measurement_uncertain() {
        for leap in [LeapIndicator::InsertSecond, LeapIndicator::DeleteSecond] {
            assert_eq!(
                evaluate(HealthInput::new(true, 1, leap, 0.0)),
                HealthStatus::Uncertain
            );
        }
    }

    #[test]
    fn leap_alarm_is_unavailable() {
        assert_eq!(
            evaluate(HealthInput::new(true, 1, LeapIndicator::Alarm, 0.0)),
            HealthStatus::Unavailable
        );
    }

    #[test]
    fn uncertain_root_distance_boundary_is_inclusive() {
        assert_eq!(
            evaluate(HealthInput::new(
                true,
                1,
                LeapIndicator::NoWarning,
                UNCERTAIN_ROOT_DISTANCE,
            )),
            HealthStatus::Uncertain
        );
    }

    #[test]
    fn excessive_negative_or_non_finite_root_distance_is_unavailable() {
        for root_distance in [
            -0.001,
            UNCERTAIN_ROOT_DISTANCE + 0.001,
            f64::NAN,
            f64::INFINITY,
        ] {
            assert_eq!(
                evaluate(HealthInput::new(
                    true,
                    1,
                    LeapIndicator::NoWarning,
                    root_distance,
                )),
                HealthStatus::Unavailable
            );
        }
    }

    #[test]
    fn status_helpers_match_variants() {
        assert!(HealthStatus::Synchronized.is_available());
        assert!(HealthStatus::Synchronized.is_synchronized());
        assert!(HealthStatus::Uncertain.is_available());
        assert!(!HealthStatus::Uncertain.is_synchronized());
        assert!(!HealthStatus::Unavailable.is_available());
        assert!(!HealthStatus::Unavailable.is_synchronized());
    }

    #[test]
    fn leap_indicator_bits_are_typed() {
        assert_eq!(LeapIndicator::from_bits(0), Some(LeapIndicator::NoWarning));
        assert_eq!(
            LeapIndicator::from_bits(1),
            Some(LeapIndicator::InsertSecond)
        );
        assert_eq!(
            LeapIndicator::from_bits(2),
            Some(LeapIndicator::DeleteSecond)
        );
        assert_eq!(LeapIndicator::from_bits(3), Some(LeapIndicator::Alarm));
        assert_eq!(LeapIndicator::from_bits(4), None);
    }
}
