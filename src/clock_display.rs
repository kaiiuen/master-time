//! UI-independent formatting for clock and Unix epoch values.
//!
//! The standard library does not expose a local-timezone database. Callers that
//! select [`TimeZone::Local`] should therefore provide the local UTC offset
//! (including DST, if applicable) with [`ClockDisplayModel::with_local_offset`].

use std::time::{Duration, SystemTime, UNIX_EPOCH};

const UNAVAILABLE: &str = "<unavailable>";
const SECONDS_PER_DAY: i64 = 86_400;

/// The timezone used for a clock display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeZone {
    /// Coordinated Universal Time, with no timezone offset.
    Utc,
    /// The local timezone, resolved by the caller.
    Local,
}

/// The hour representation used by a clock display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HourFormat {
    /// `00:00:00` through `23:59:59`.
    TwentyFourHour,
    /// `12:00:00 AM` through `11:59:59 PM`.
    TwelveHour,
}

/// Which value should be rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    /// A calendar date and time.
    Clock,
    /// Signed seconds since 1970-01-01 00:00:00 UTC.
    UnixEpoch,
}

/// A configured, UI-independent clock display.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClockDisplayModel {
    timestamp: Option<SystemTime>,
    timezone: TimeZone,
    hour_format: HourFormat,
    mode: DisplayMode,
    measurement_offset_seconds: Option<f64>,
    local_offset_seconds: Option<i32>,
}

impl ClockDisplayModel {
    /// Creates a UTC, 24-hour calendar-clock display.
    pub const fn new(timestamp: Option<SystemTime>) -> Self {
        Self {
            timestamp,
            timezone: TimeZone::Utc,
            hour_format: HourFormat::TwentyFourHour,
            mode: DisplayMode::Clock,
            measurement_offset_seconds: None,
            local_offset_seconds: None,
        }
    }

    pub const fn timestamp(self) -> Option<SystemTime> {
        self.timestamp
    }

    pub const fn timezone(self) -> TimeZone {
        self.timezone
    }

    pub const fn hour_format(self) -> HourFormat {
        self.hour_format
    }

    pub const fn mode(self) -> DisplayMode {
        self.mode
    }

    pub const fn with_timezone(mut self, timezone: TimeZone) -> Self {
        self.timezone = timezone;
        self
    }

    pub const fn with_hour_format(mut self, hour_format: HourFormat) -> Self {
        self.hour_format = hour_format;
        self
    }

    pub const fn with_display_mode(mut self, mode: DisplayMode) -> Self {
        self.mode = mode;
        self
    }

    /// Supplies the local offset in seconds east of UTC, such as `-18_000`
    /// for US Eastern Standard Time. It is ignored for [`TimeZone::Utc`].
    pub const fn with_local_offset(mut self, offset_seconds: i32) -> Self {
        self.local_offset_seconds = Some(offset_seconds);
        self
    }

    /// Adds a measured clock correction before formatting.
    ///
    /// Non-finite offsets are treated as unavailable. This is intentionally an
    /// `f64` because NTP measurements in this application are fractional
    /// seconds.
    pub const fn with_measurement_offset(mut self, offset_seconds: Option<f64>) -> Self {
        self.measurement_offset_seconds = offset_seconds;
        self
    }

    /// Formats the configured value, or [`UNAVAILABLE`] when it cannot be
    /// represented safely.
    pub fn format(&self) -> String {
        let Some(timestamp) = self.timestamp else {
            return UNAVAILABLE.to_owned();
        };
        let Some(epoch_seconds) = epoch_seconds(timestamp) else {
            return UNAVAILABLE.to_owned();
        };
        let correction = self.measurement_offset_seconds.unwrap_or(0.0);
        if !correction.is_finite() {
            return UNAVAILABLE.to_owned();
        }
        let corrected = epoch_seconds as f64 + correction;
        if !corrected.is_finite() || corrected.abs() > i64::MAX as f64 {
            return UNAVAILABLE.to_owned();
        }

        match self.mode {
            DisplayMode::UnixEpoch => format_epoch(corrected),
            DisplayMode::Clock => {
                let offset = match self.timezone {
                    TimeZone::Utc => 0,
                    TimeZone::Local => match self.local_offset_seconds {
                        Some(offset) => i64::from(offset),
                        None => return UNAVAILABLE.to_owned(),
                    },
                };
                format_clock(corrected, offset, self.hour_format)
                    .unwrap_or_else(|| UNAVAILABLE.to_owned())
            }
        }
    }
}

fn epoch_seconds(timestamp: SystemTime) -> Option<i64> {
    match timestamp.duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_secs()).ok(),
        Err(error) => {
            let seconds = i64::try_from(error.duration().as_secs()).ok()?;
            seconds.checked_neg()
        }
    }
}

fn format_epoch(seconds: f64) -> String {
    if seconds.fract() == 0.0 {
        format!("{seconds:.0}")
    } else {
        format!("{seconds:.3}")
    }
}

fn format_clock(seconds: f64, offset: i64, hour_format: HourFormat) -> Option<String> {
    let whole_seconds = (seconds.floor() as i64).checked_add(offset)?;
    let day = whole_seconds.div_euclid(SECONDS_PER_DAY);
    let in_day = whole_seconds.rem_euclid(SECONDS_PER_DAY);
    let hour = (in_day / 3_600) as u32;
    let minute = ((in_day % 3_600) / 60) as u32;
    let second = (in_day % 60) as u32;
    let (year, month, day) = civil_from_days(day);

    match hour_format {
        HourFormat::TwentyFourHour => Some(format!(
            "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}"
        )),
        HourFormat::TwelveHour => {
            let meridiem = if hour < 12 { "AM" } else { "PM" };
            let display_hour = match hour % 12 {
                0 => 12,
                hour => hour,
            };
            Some(format!(
                "{year:04}-{month:02}-{day:02} {display_hour:02}:{minute:02}:{second:02} {meridiem}"
            ))
        }
    }
}

// Proleptic Gregorian conversion, adapted from the public-domain civil-date
// algorithm. It uses Euclidean division so dates before the Unix epoch work.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    (
        year + if month <= 2 { 1 } else { 0 },
        month as u32,
        day as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timestamp(seconds: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(seconds)
    }

    #[test]
    fn formats_utc_in_24_hour_mode() {
        let model = ClockDisplayModel::new(Some(timestamp(1_704_067_200)));
        assert_eq!(model.format(), "2024-01-01 00:00:00");
    }

    #[test]
    fn formats_local_in_12_hour_mode() {
        let model = ClockDisplayModel::new(Some(timestamp(1_704_067_200)))
            .with_timezone(TimeZone::Local)
            .with_local_offset(-5 * 3_600)
            .with_hour_format(HourFormat::TwelveHour);
        assert_eq!(model.format(), "2023-12-31 07:00:00 PM");
    }

    #[test]
    fn formats_epoch_and_measurement_offset() {
        let model = ClockDisplayModel::new(Some(timestamp(10)))
            .with_display_mode(DisplayMode::UnixEpoch)
            .with_measurement_offset(Some(-0.25));
        assert_eq!(model.format(), "9.750");
    }

    #[test]
    fn formats_pre_epoch_values() {
        let model = ClockDisplayModel::new(Some(UNIX_EPOCH - Duration::from_secs(1)));
        assert_eq!(model.format(), "1969-12-31 23:59:59");
    }

    #[test]
    fn unavailable_states_are_safe() {
        assert_eq!(ClockDisplayModel::new(None).format(), UNAVAILABLE);
        assert_eq!(
            ClockDisplayModel::new(Some(timestamp(0)))
                .with_timezone(TimeZone::Local)
                .format(),
            UNAVAILABLE
        );
        assert_eq!(
            ClockDisplayModel::new(Some(timestamp(0)))
                .with_measurement_offset(Some(f64::NAN))
                .format(),
            UNAVAILABLE
        );
    }
}
