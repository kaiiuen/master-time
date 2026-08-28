//! Platform boundary for applying an approved clock correction.
//!
//! This module deliberately does not connect the correction planner to the
//! operating system. Callers must explicitly opt in before an application is
//! attempted. [`PlatformTimeAdapter::preview`] is always a dry run and never
//! invokes a platform API.
//!
//! On Windows, the opted-in path uses the documented `SetSystemTime` API. The
//! process must have the Windows `SeSystemtimePrivilege`; a denied call is
//! reported as [`PlatformTimeError::PermissionDenied`]. Other platforms return
//! [`PlatformTimeError::UnsupportedPlatform`] rather than attempting a shell
//! command or another best-effort clock change.

use crate::time_action::ApprovedCorrection;
use std::fmt;

/// A side-effect-free description of the operation that would be attempted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CorrectionDryRun {
    /// The approved correction in seconds.
    pub offset: f64,
    /// Whether this preview is guaranteed not to change the clock.
    pub dry_run: bool,
}

/// The result of an actually applied correction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AppliedCorrection {
    /// The correction requested from the platform.
    pub offset: f64,
}

/// Errors returned by the platform boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformTimeError {
    /// The caller did not explicitly enable clock changes.
    OptInRequired,
    /// This target has no supported clock-setting backend.
    UnsupportedPlatform,
    /// The correction cannot be represented safely as a system time.
    InvalidCorrection,
    /// Windows rejected the operation because the process lacks permission.
    PermissionDenied,
    /// Windows failed for a reason other than insufficient permission.
    PlatformFailure(u32),
}

impl fmt::Display for PlatformTimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OptInRequired => formatter.write_str(
                "setting the system clock is disabled; explicit platform opt-in is required",
            ),
            Self::UnsupportedPlatform => {
                formatter.write_str("setting the system clock is unsupported on this platform")
            }
            Self::InvalidCorrection => formatter.write_str(
                "the approved clock correction is not a finite, representable system time",
            ),
            Self::PermissionDenied => formatter.write_str(
                "Windows denied the clock change; enable the SeSystemtimePrivilege permission",
            ),
            Self::PlatformFailure(code) => {
                write!(
                    formatter,
                    "Windows rejected the clock change (error {code})"
                )
            }
        }
    }
}

impl std::error::Error for PlatformTimeError {}

/// A guarded adapter for an approved correction from [`crate::time_action`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PlatformTimeAdapter {
    opted_in: bool,
}

impl PlatformTimeAdapter {
    /// Creates a disabled adapter. This is the safe default.
    pub const fn new() -> Self {
        Self { opted_in: false }
    }

    /// Creates an adapter explicitly authorized to attempt clock changes.
    ///
    /// Prefer keeping the returned value scoped to the user-confirmed action.
    /// Constructing this value is the opt-in event; no platform operation is
    /// performed here.
    pub const fn with_explicit_opt_in() -> Self {
        Self { opted_in: true }
    }

    /// Returns whether this adapter was explicitly authorized to apply changes.
    pub const fn is_opted_in(self) -> bool {
        self.opted_in
    }

    /// Describes an approved correction without touching the system clock.
    pub fn preview(
        &self,
        correction: ApprovedCorrection,
    ) -> Result<CorrectionDryRun, PlatformTimeError> {
        validate(correction)?;
        Ok(CorrectionDryRun {
            offset: correction.offset,
            dry_run: true,
        })
    }

    /// Applies an approved correction, subject to opt-in and platform guards.
    pub fn apply(
        &self,
        correction: ApprovedCorrection,
    ) -> Result<AppliedCorrection, PlatformTimeError> {
        validate(correction)?;

        if !self.opted_in {
            return Err(PlatformTimeError::OptInRequired);
        }

        platform::apply(correction)
    }
}

fn validate(correction: ApprovedCorrection) -> Result<(), PlatformTimeError> {
    if correction.offset.is_finite() {
        Ok(())
    } else {
        Err(PlatformTimeError::InvalidCorrection)
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;

    pub(super) fn apply(_: ApprovedCorrection) -> Result<AppliedCorrection, PlatformTimeError> {
        Err(PlatformTimeError::UnsupportedPlatform)
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[repr(C)]
    struct WindowsSystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLastError() -> u32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetSystemTime(system_time: *const WindowsSystemTime) -> i32;
    }

    pub(super) fn apply(
        correction: ApprovedCorrection,
    ) -> Result<AppliedCorrection, PlatformTimeError> {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| PlatformTimeError::InvalidCorrection)?
            .as_secs_f64()
            + correction.offset;
        let system_time = utc_system_time(seconds)?;

        // This is the sole operating-system side effect in this module.
        let succeeded = unsafe { SetSystemTime(&system_time) } != 0;
        if succeeded {
            Ok(AppliedCorrection {
                offset: correction.offset,
            })
        } else {
            let error = unsafe { GetLastError() };
            if error == 5 {
                Err(PlatformTimeError::PermissionDenied)
            } else {
                Err(PlatformTimeError::PlatformFailure(error))
            }
        }
    }

    fn utc_system_time(seconds: f64) -> Result<WindowsSystemTime, PlatformTimeError> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(PlatformTimeError::InvalidCorrection);
        }
        let whole_seconds = seconds.floor();
        if whole_seconds > u64::MAX as f64 {
            return Err(PlatformTimeError::InvalidCorrection);
        }
        let days = (whole_seconds as u64) / 86_400;
        let day_seconds = (whole_seconds as u64) % 86_400;
        let (year, month, day) = civil_date(days)?;
        Ok(WindowsSystemTime {
            year,
            month,
            day_of_week: 0,
            day,
            hour: (day_seconds / 3_600) as u16,
            minute: ((day_seconds % 3_600) / 60) as u16,
            second: (day_seconds % 60) as u16,
            milliseconds: ((seconds - whole_seconds) * 1_000.0) as u16,
        })
    }

    // Howard Hinnant's civil-from-days algorithm, expressed without a date
    // dependency so the FFI boundary remains the only Windows-specific code.
    fn civil_date(days_since_epoch: u64) -> Result<(u16, u16, u16), PlatformTimeError> {
        let z = i64::try_from(days_since_epoch)
            .map_err(|_| PlatformTimeError::InvalidCorrection)?
            + 719_468;
        let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let year = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let month = (5 * doy + 2) / 153;
        let day = doy - (153 * month + 2) / 5 + 1;
        let year = year + if month >= 10 { 1 } else { 0 };
        let month = month + if month < 10 { 3 } else { -9 };
        Ok((
            u16::try_from(year).map_err(|_| PlatformTimeError::InvalidCorrection)?,
            u16::try_from(month).map_err(|_| PlatformTimeError::InvalidCorrection)?,
            u16::try_from(day).map_err(|_| PlatformTimeError::InvalidCorrection)?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn correction(offset: f64) -> ApprovedCorrection {
        ApprovedCorrection { offset }
    }

    #[test]
    fn default_adapter_refuses_to_apply() {
        let error = PlatformTimeAdapter::new().apply(correction(1.0));
        assert_eq!(error, Err(PlatformTimeError::OptInRequired));
    }

    #[test]
    fn preview_is_dry_run_and_does_not_require_opt_in() {
        let preview = PlatformTimeAdapter::new()
            .preview(correction(-2.5))
            .unwrap();
        assert_eq!(preview.offset, -2.5);
        assert!(preview.dry_run);
    }

    #[test]
    fn non_finite_corrections_are_refused() {
        assert_eq!(
            PlatformTimeAdapter::new().preview(correction(f64::NAN)),
            Err(PlatformTimeError::InvalidCorrection)
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn opted_in_apply_reports_unsupported_platform() {
        assert_eq!(
            PlatformTimeAdapter::with_explicit_opt_in().apply(correction(1.0)),
            Err(PlatformTimeError::UnsupportedPlatform)
        );
    }
}
