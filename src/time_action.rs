//! UI-independent planning for requested clock corrections.
//!
//! This module does not read or change the system clock. It turns a measured
//! offset into a preview and, when [`SyncPolicy`] permits it, a typed request
//! for a future platform adapter to apply.

use crate::health::HealthStatus;
use crate::measurement::Measurement;
use crate::sync_policy::{SyncDisposition, SyncPolicy};
use std::fmt;

/// The information needed to consider one requested clock correction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CorrectionRequest {
    /// The measured offset, if the exchange produced one.
    pub measurement: Option<Measurement>,
    /// The health classification associated with the response.
    pub health: HealthStatus,
    /// The NTP stratum associated with the response.
    pub stratum: u8,
}

impl CorrectionRequest {
    pub const fn new(measurement: Option<Measurement>, health: HealthStatus, stratum: u8) -> Self {
        Self {
            measurement,
            health,
            stratum,
        }
    }
}

/// A side-effect-free summary suitable for displaying before confirmation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CorrectionPreview {
    /// The measured clock offset in seconds, when a measurement is available.
    pub offset: Option<f64>,
    /// The disposition returned by the synchronization policy.
    pub disposition: SyncDisposition,
}

/// An approved correction for a future platform adapter to apply.
///
/// Constructing this value never changes the system clock. The adapter owns
/// the eventual platform-specific operation and may apply its own safeguards.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ApprovedCorrection {
    /// The amount by which the local clock is behind (positive) or ahead
    /// (negative), in seconds.
    pub offset: f64,
}

/// Why a requested correction was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorrectionRefusal {
    /// No complete measurement was available.
    MeasurementUnavailable,
    /// The measurement may be displayed but is not safe to apply.
    DisplayOnly,
    /// The measurement failed the policy's safety checks.
    Unsafe,
}

impl fmt::Display for CorrectionRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MeasurementUnavailable => "clock correction measurement is unavailable",
            Self::DisplayOnly => "clock correction is display-only",
            Self::Unsafe => "clock correction is unsafe",
        })
    }
}

impl std::error::Error for CorrectionRefusal {}

/// The result of handling a requested correction.
///
/// The approved variant is intentionally just data. A future platform adapter
/// can consume it without this model knowing how, or whether, to set a clock.
pub type CorrectionResult = Result<ApprovedCorrection, CorrectionRefusal>;

/// UI-independent action model for requested clock corrections.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeAction {
    policy: SyncPolicy,
}

impl TimeAction {
    /// Creates an action model backed by `policy`.
    pub const fn new(policy: SyncPolicy) -> Self {
        Self { policy }
    }

    /// Returns the policy preview without performing any side effect.
    pub fn preview(&self, request: CorrectionRequest) -> CorrectionPreview {
        CorrectionPreview {
            offset: request.measurement.map(|measurement| measurement.offset),
            disposition: self
                .policy
                .classify(request.measurement, request.health, request.stratum),
        }
    }

    /// Handles a correction request without changing the system clock.
    pub fn request_correction(&self, request: CorrectionRequest) -> CorrectionResult {
        let preview = self.preview(request);

        if request.measurement.is_none() {
            return Err(CorrectionRefusal::MeasurementUnavailable);
        }

        match preview.disposition {
            SyncDisposition::EligibleForCorrection => Ok(ApprovedCorrection {
                // The eligible policy path guarantees a finite offset.
                offset: preview.offset.expect("measurement was checked above"),
            }),
            SyncDisposition::DisplayOnly => Err(CorrectionRefusal::DisplayOnly),
            SyncDisposition::Unsafe => Err(CorrectionRefusal::Unsafe),
        }
    }

    pub const fn policy(&self) -> SyncPolicy {
        self.policy
    }
}

impl Default for TimeAction {
    fn default() -> Self {
        Self::new(SyncPolicy::default())
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

    fn request(
        measurement: Option<Measurement>,
        health: HealthStatus,
        stratum: u8,
    ) -> CorrectionRequest {
        CorrectionRequest::new(measurement, health, stratum)
    }

    fn synchronized(root_distance: f64) -> HealthStatus {
        evaluate(HealthInput::new(
            true,
            2,
            LeapIndicator::NoWarning,
            root_distance,
        ))
    }

    #[test]
    fn eligible_request_exposes_preview_and_approved_offset() {
        let action = TimeAction::new(SyncPolicy::new(0.5));
        let request = request(Some(measurement(0.25, 0.25)), synchronized(0.25), 2);

        assert_eq!(
            action.preview(request),
            CorrectionPreview {
                offset: Some(0.25),
                disposition: SyncDisposition::EligibleForCorrection,
            }
        );
        assert_eq!(
            action.request_correction(request),
            Ok(ApprovedCorrection { offset: 0.25 })
        );
    }

    #[test]
    fn display_only_request_is_refused() {
        let action = TimeAction::new(SyncPolicy::new(0.5));
        let request = request(Some(measurement(0.25, 2.0)), HealthStatus::Uncertain, 2);

        assert_eq!(
            action.preview(request).disposition,
            SyncDisposition::DisplayOnly
        );
        assert_eq!(
            action.request_correction(request),
            Err(CorrectionRefusal::DisplayOnly)
        );
    }

    #[test]
    fn unsafe_request_is_refused() {
        let action = TimeAction::default();
        let request = request(Some(measurement(2.0, 0.25)), synchronized(0.25), 2);

        assert_eq!(action.preview(request).disposition, SyncDisposition::Unsafe);
        assert_eq!(
            action.request_correction(request),
            Err(CorrectionRefusal::Unsafe)
        );
    }

    #[test]
    fn unavailable_measurement_is_refused_explicitly() {
        let action = TimeAction::default();
        let request = request(None, HealthStatus::Unavailable, 2);

        assert_eq!(
            action.preview(request),
            CorrectionPreview {
                offset: None,
                disposition: SyncDisposition::Unsafe,
            }
        );
        assert_eq!(
            action.request_correction(request),
            Err(CorrectionRefusal::MeasurementUnavailable)
        );
    }
}
