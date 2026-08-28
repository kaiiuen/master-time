//! Framework-neutral presentation models for the desktop application.
//!
//! This module deliberately owns only display data. It does not know about a
//! widget framework, scheduling, networking, or persistence. A future UI can
//! render [`Presentation`] without needing to format domain values itself.

use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::health::{self, HealthInput, HealthStatus, LeapIndicator};
use crate::measurement::MeasurementHistory;
use crate::service::{MeasurementResult, ServiceError};

const UNAVAILABLE: &str = "Unavailable";

/// The input snapshot consumed by [`present`].
#[derive(Debug, Default)]
pub struct ApplicationState<'a> {
    /// The local time to display. Supplying a captured value keeps rendering
    /// deterministic and avoids making the presentation layer read the clock.
    pub current_time: Option<SystemTime>,
    /// The most recent successful NTP exchange, if one exists.
    pub measurement: Option<&'a MeasurementResult>,
    /// The rolling offset history, if history is enabled by the application.
    pub history: Option<&'a MeasurementHistory>,
    /// The most recent failure, if the last poll failed.
    pub error: Option<&'a ServiceError>,
}

/// All stable sections presented by the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Presentation {
    pub current_time: CurrentTimeView,
    pub server_metrics: ServerMetricsView,
    pub status: StatusView,
    pub errors: ErrorView,
    pub history: HistoryView,
}

/// Current local time section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentTimeView {
    pub value: String,
}

/// Server and measurement metrics section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerMetricsView {
    pub server: String,
    pub stratum: String,
    pub offset: String,
    pub round_trip_delay: String,
    pub root_distance: String,
}

/// The coarse status rendered by the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Synchronized,
    Uncertain,
    Unavailable,
}

/// Status section, including a short human-readable explanation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusView {
    pub kind: StatusKind,
    pub label: String,
    pub detail: String,
}

/// Error section. An empty message means that no error is currently shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorView {
    pub message: Option<String>,
}

/// History section, newest sample first in the rendered view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryView {
    pub samples: Vec<String>,
    pub summary: Option<String>,
}

/// Converts application state into the complete presentation boundary.
pub fn present(state: ApplicationState<'_>) -> Presentation {
    let current_time = CurrentTimeView {
        value: state
            .current_time
            .map(format_current_time)
            .unwrap_or_else(|| UNAVAILABLE.to_owned()),
    };
    let server_metrics = state
        .measurement
        .map(format_server_metrics)
        .unwrap_or_else(ServerMetricsView::unavailable);
    let status = state
        .measurement
        .map(format_status)
        .unwrap_or_else(|| unavailable_status("No successful measurement"));
    let errors = ErrorView {
        message: state.error.map(format_error),
    };
    let history = state
        .history
        .map(format_history)
        .unwrap_or_else(HistoryView::unavailable);

    Presentation {
        current_time,
        server_metrics,
        status,
        errors,
        history,
    }
}

/// Formats a timestamp as stable UTC text: `YYYY-MM-DD HH:MM:SS.mmm UTC`.
pub fn format_current_time(time: SystemTime) -> String {
    let Ok(duration) = time.duration_since(UNIX_EPOCH) else {
        return UNAVAILABLE.to_owned();
    };
    let seconds = duration.as_secs();
    let days = seconds / 86_400;
    let seconds_today = seconds % 86_400;
    let (year, month, day) = civil_date(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}.{:03} UTC",
        seconds_today / 3_600,
        (seconds_today / 60) % 60,
        seconds_today % 60,
        duration.subsec_millis()
    )
}

/// Formats a duration in milliseconds, retaining a sign for offsets.
pub fn format_milliseconds(seconds: f64) -> String {
    if !seconds.is_finite() {
        return UNAVAILABLE.to_owned();
    }
    format!("{:+.2} ms", seconds * 1_000.0)
}

/// Formats server identity, protocol quality, and measurement values.
pub fn format_server_metrics(result: &MeasurementResult) -> ServerMetricsView {
    ServerMetricsView {
        server: format_socket(result.server),
        stratum: result.header.stratum.to_string(),
        offset: format_milliseconds(result.measurement.offset),
        round_trip_delay: format_milliseconds(result.measurement.round_trip_delay),
        root_distance: format_milliseconds(result.measurement.root_distance),
    }
}

/// Maps a successful measurement to the health status shown to users.
pub fn format_status(result: &MeasurementResult) -> StatusView {
    let leap =
        LeapIndicator::from_bits(result.header.leap_indicator).unwrap_or(LeapIndicator::Alarm);
    let health = health::evaluate(HealthInput::new(
        true,
        result.header.stratum,
        leap,
        result.measurement.root_distance,
    ));
    match health {
        HealthStatus::Synchronized => StatusView {
            kind: StatusKind::Synchronized,
            label: "Synchronized".to_owned(),
            detail: "Server time is within the configured health thresholds".to_owned(),
        },
        HealthStatus::Uncertain => StatusView {
            kind: StatusKind::Uncertain,
            label: "Uncertain".to_owned(),
            detail: "Measurement is available but synchronization quality needs attention"
                .to_owned(),
        },
        HealthStatus::Unavailable => {
            unavailable_status("Measurement cannot establish clock health")
        }
    }
}

fn unavailable_status(detail: &str) -> StatusView {
    StatusView {
        kind: StatusKind::Unavailable,
        label: UNAVAILABLE.to_owned(),
        detail: detail.to_owned(),
    }
}

/// Formats offset samples newest-first and summarizes their statistics.
pub fn format_history(history: &MeasurementHistory) -> HistoryView {
    let samples: Vec<String> = history
        .samples()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|sample| format_milliseconds(*sample))
        .collect();
    let summary = history.statistics().map(|stats| {
        format!(
            "min {}, mean {}, max {}, deviation {}",
            format_milliseconds(stats.min),
            format_milliseconds(stats.mean),
            format_milliseconds(stats.max),
            format_milliseconds(stats.standard_deviation),
        )
    });
    HistoryView { samples, summary }
}

/// Formats the latest polling error for the error section.
pub fn format_error(error: &ServiceError) -> String {
    error.to_string()
}

fn format_socket(server: SocketAddr) -> String {
    server.to_string()
}

impl ServerMetricsView {
    fn unavailable() -> Self {
        Self {
            server: UNAVAILABLE.to_owned(),
            stratum: UNAVAILABLE.to_owned(),
            offset: UNAVAILABLE.to_owned(),
            round_trip_delay: UNAVAILABLE.to_owned(),
            root_distance: UNAVAILABLE.to_owned(),
        }
    }
}

impl HistoryView {
    fn unavailable() -> Self {
        Self {
            samples: Vec::new(),
            summary: None,
        }
    }
}

// Howard Hinnant's civil-from-days algorithm, using only the standard library.
fn civil_date(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    (year + i64::from(month <= 2), month, day)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn formats_epoch_and_milliseconds_stably() {
        assert_eq!(
            format_current_time(UNIX_EPOCH),
            "1970-01-01 00:00:00.000 UTC"
        );
        assert_eq!(
            format_current_time(UNIX_EPOCH + Duration::from_millis(3_723_456)),
            "1970-01-01 01:02:03.456 UTC"
        );
        assert_eq!(format_milliseconds(-0.0123), "-12.30 ms");
    }

    #[test]
    fn formats_unavailable_inputs_without_sentinels_or_panics() {
        assert_eq!(
            format_current_time(UNIX_EPOCH - Duration::from_secs(1)),
            UNAVAILABLE
        );
        assert_eq!(format_milliseconds(f64::NAN), UNAVAILABLE);
        let presentation = present(ApplicationState::default());
        assert_eq!(presentation.current_time.value, UNAVAILABLE);
        assert_eq!(presentation.server_metrics.server, UNAVAILABLE);
        assert_eq!(presentation.status.kind, StatusKind::Unavailable);
        assert!(presentation.errors.message.is_none());
        assert!(presentation.history.samples.is_empty());
        assert!(presentation.history.summary.is_none());
    }

    #[test]
    fn history_is_newest_first_and_has_a_stable_summary() {
        let mut history = MeasurementHistory::new(4);
        history.push(0.001);
        history.push(-0.002);
        let view = format_history(&history);
        assert_eq!(view.samples, vec!["-2.00 ms", "+1.00 ms"]);
        assert_eq!(
            view.summary.as_deref(),
            Some("min -2.00 ms, mean -0.50 ms, max +1.00 ms, deviation 1.50 ms")
        );
    }
}
