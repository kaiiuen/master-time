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

const FILETIME_TICKS_PER_SECOND: f64 = 10_000_000.0;
// The largest FILETIME that can be converted to SYSTEMTIME (year 9999).
const MAX_SYSTEMTIME_FILETIME_TICKS: u64 = 2_650_467_743_999_999_999;

/// Adds a correction expressed in seconds to a UTC FILETIME tick count.
///
/// FILETIME is an unsigned count of 100-nanosecond intervals since 1601-01-01
/// UTC. Keep the conversion checked: Rust's float-to-integer casts saturate,
/// which would otherwise turn an unrepresentable correction into a valid but
/// incorrect timestamp.
fn adjusted_file_time_ticks(
    current_ticks: u64,
    offset_seconds: f64,
) -> Result<u64, PlatformTimeError> {
    if !offset_seconds.is_finite() {
        return Err(PlatformTimeError::InvalidCorrection);
    }

    let offset_ticks = offset_seconds * FILETIME_TICKS_PER_SECOND;
    if !offset_ticks.is_finite() || offset_ticks.abs() > u64::MAX as f64 {
        return Err(PlatformTimeError::InvalidCorrection);
    }

    // The range check above keeps this cast well inside i128's range. Truncate
    // sub-tick precision deterministically; SYSTEMTIME itself has milliseconds
    // as its finest documented resolution.
    let offset_ticks = offset_ticks.trunc() as i128;
    let target_ticks = i128::from(current_ticks)
        .checked_add(offset_ticks)
        .and_then(|ticks| u64::try_from(ticks).ok())
        .ok_or(PlatformTimeError::InvalidCorrection)?;

    if target_ticks > MAX_SYSTEMTIME_FILETIME_TICKS {
        Err(PlatformTimeError::InvalidCorrection)
    } else {
        Ok(target_ticks)
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
    const ERROR_ACCESS_DENIED: u32 = 5;
    const ERROR_PRIVILEGE_NOT_HELD: u32 = 1_314;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
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

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct WindowsFileTime {
        low_date_time: u32,
        high_date_time: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLastError() -> u32;
        fn GetSystemTime(system_time: *mut WindowsSystemTime);
        fn SystemTimeToFileTime(
            system_time: *const WindowsSystemTime,
            file_time: *mut WindowsFileTime,
        ) -> i32;
        fn FileTimeToSystemTime(
            file_time: *const WindowsFileTime,
            system_time: *mut WindowsSystemTime,
        ) -> i32;
        fn SetSystemTime(system_time: *const WindowsSystemTime) -> i32;
    }

    pub(super) fn apply(
        correction: ApprovedCorrection,
    ) -> Result<AppliedCorrection, PlatformTimeError> {
        let mut current_utc = WindowsSystemTime::default();
        unsafe { GetSystemTime(&mut current_utc) };

        let mut current_file_time = WindowsFileTime::default();
        if unsafe { SystemTimeToFileTime(&current_utc, &mut current_file_time) } == 0 {
            return Err(PlatformTimeError::PlatformFailure(unsafe {
                GetLastError()
            }));
        }

        let current_ticks = file_time_ticks(current_file_time);
        let target_ticks = super::adjusted_file_time_ticks(current_ticks, correction.offset)?;
        let target_file_time = WindowsFileTime {
            low_date_time: target_ticks as u32,
            high_date_time: (target_ticks >> 32) as u32,
        };
        let mut target_utc = WindowsSystemTime::default();
        if unsafe { FileTimeToSystemTime(&target_file_time, &mut target_utc) } == 0 {
            return Err(PlatformTimeError::PlatformFailure(unsafe {
                GetLastError()
            }));
        }

        // This is the sole operating-system side effect in this module. The
        // SYSTEMTIME passed to SetSystemTime is explicitly UTC.
        if unsafe { SetSystemTime(&target_utc) } != 0 {
            Ok(AppliedCorrection {
                offset: correction.offset,
            })
        } else {
            let error = unsafe { GetLastError() };
            if matches!(error, ERROR_ACCESS_DENIED | ERROR_PRIVILEGE_NOT_HELD) {
                Err(PlatformTimeError::PermissionDenied)
            } else {
                Err(PlatformTimeError::PlatformFailure(error))
            }
        }
    }

    fn file_time_ticks(file_time: WindowsFileTime) -> u64 {
        (u64::from(file_time.high_date_time) << 32) | u64::from(file_time.low_date_time)
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

        let opted_in_preview = PlatformTimeAdapter::with_explicit_opt_in()
            .preview(correction(1.25))
            .unwrap();
        assert_eq!(opted_in_preview.offset, 1.25);
        assert!(opted_in_preview.dry_run);
    }

    #[test]
    fn non_finite_corrections_are_refused() {
        for offset in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                PlatformTimeAdapter::new().preview(correction(offset)),
                Err(PlatformTimeError::InvalidCorrection)
            );
            assert_eq!(
                PlatformTimeAdapter::with_explicit_opt_in().apply(correction(offset)),
                Err(PlatformTimeError::InvalidCorrection)
            );
        }
    }

    #[test]
    fn filetime_adjustment_is_checked_and_deterministic() {
        assert_eq!(
            super::adjusted_file_time_ticks(1_000_000, 0.25),
            Ok(3_500_000)
        );
        assert_eq!(
            super::adjusted_file_time_ticks(1_000_000, -0.10000001),
            Ok(0)
        );
        assert_eq!(
            super::adjusted_file_time_ticks(0, -0.0000001),
            Err(PlatformTimeError::InvalidCorrection)
        );
        assert_eq!(
            super::adjusted_file_time_ticks(super::MAX_SYSTEMTIME_FILETIME_TICKS, 0.0000001),
            Err(PlatformTimeError::InvalidCorrection)
        );
        assert_eq!(
            super::adjusted_file_time_ticks(u64::MAX, -0.0000001),
            Err(PlatformTimeError::InvalidCorrection)
        );
        assert_eq!(
            super::adjusted_file_time_ticks(0, f64::MAX),
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
