//! UI-independent presentation of application diagnostics.
//!
//! The model contains a fixed set of labeled rows so a UI can render the
//! diagnostics without knowing how values are collected or formatted.

use std::time::Duration;

use crate::measurement::Measurement;
use crate::{DiagnosticsSnapshot, HealthStatus};

const UNAVAILABLE: &str = "unavailable";

/// One labeled value in the diagnostics presentation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticsRow {
    pub label: &'static str,
    pub value: String,
}

/// Stable, UI-independent diagnostics presentation data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticsView {
    rows: Vec<DiagnosticsRow>,
}

impl DiagnosticsView {
    /// Builds the diagnostics rows from platform and measurement data.
    ///
    /// The returned rows always have the same order and labels. Measurement
    /// fields and stratum are unavailable when there is no current result;
    /// status is still shown because it describes the current health state.
    pub fn new(
        snapshot: &DiagnosticsSnapshot,
        measurement: Option<Measurement>,
        stratum: Option<u8>,
        status: HealthStatus,
    ) -> Self {
        let measurement = measurement.filter(|measurement| {
            measurement.offset.is_finite()
                && measurement.round_trip_delay.is_finite()
                && measurement.root_distance.is_finite()
        });

        Self {
            rows: vec![
                row("Uptime", format_duration(snapshot.uptime)),
                row(
                    "CPU count",
                    format_option(snapshot.logical_cpu_count, |count| count.to_string()),
                ),
                row(
                    "CPU utilization",
                    format_option(
                        snapshot.cpu_usage_percent.filter(|value| value.is_finite()),
                        |usage| format!("{usage:.1}%"),
                    ),
                ),
                row(
                    "Current offset",
                    format_measurement(measurement.map(|value| value.offset)),
                ),
                row(
                    "Delay",
                    format_measurement(measurement.map(|value| value.round_trip_delay)),
                ),
                row(
                    "Root distance",
                    format_measurement(measurement.map(|value| value.root_distance)),
                ),
                row("Stratum", format_option(stratum, |value| value.to_string())),
                row("Status", format_status(status)),
            ],
        }
    }

    /// Returns the rows in their stable display order.
    pub fn rows(&self) -> &[DiagnosticsRow] {
        &self.rows
    }
}

fn row(label: &'static str, value: String) -> DiagnosticsRow {
    DiagnosticsRow { label, value }
}

fn format_duration(duration: Option<Duration>) -> String {
    match duration {
        Some(duration) => {
            let seconds = duration.as_secs();
            format!(
                "{}d {:02}h {:02}m {:02}s",
                seconds / 86_400,
                (seconds / 3_600) % 24,
                (seconds / 60) % 60,
                seconds % 60
            )
        }
        None => UNAVAILABLE.to_owned(),
    }
}

fn format_measurement(value: Option<f64>) -> String {
    value.filter(|value| value.is_finite()).map_or_else(
        || UNAVAILABLE.to_owned(),
        |value| {
            let milliseconds = value * 1_000.0;
            let rounded = (milliseconds * 1_000.0).round() / 1_000.0;
            format!("{rounded:.3} ms")
        },
    )
}

fn format_option<T>(value: Option<T>, format: impl FnOnce(T) -> String) -> String {
    value.map_or_else(|| UNAVAILABLE.to_owned(), format)
}

fn format_status(status: HealthStatus) -> String {
    match status {
        HealthStatus::Synchronized => "Synchronized",
        HealthStatus::Uncertain => "Uncertain",
        HealthStatus::Unavailable => "Unavailable",
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(rows: &[DiagnosticsRow]) -> Vec<&'static str> {
        rows.iter().map(|row| row.label).collect()
    }

    #[test]
    fn presents_all_rows_in_a_stable_order() {
        let view = DiagnosticsView::new(
            &DiagnosticsSnapshot {
                uptime: Some(Duration::from_secs(90_061)),
                logical_cpu_count: Some(8),
                cpu_usage_percent: Some(12.34),
            },
            Some(Measurement {
                offset: 0.0012345,
                round_trip_delay: 0.0123456,
                root_distance: 0.0789012,
            }),
            Some(2),
            HealthStatus::Synchronized,
        );

        assert_eq!(
            labels(view.rows()),
            vec![
                "Uptime",
                "CPU count",
                "CPU utilization",
                "Current offset",
                "Delay",
                "Root distance",
                "Stratum",
                "Status",
            ]
        );
        assert_eq!(view.rows()[0].value, "1d 01h 01m 01s");
        assert_eq!(view.rows()[2].value, "12.3%");
        assert_eq!(view.rows()[3].value, "1.235 ms");
        assert_eq!(view.rows()[4].value, "12.346 ms");
        assert_eq!(view.rows()[5].value, "78.901 ms");
        assert_eq!(view.rows()[7].value, "Synchronized");
    }

    #[test]
    fn renders_missing_values_as_unavailable() {
        let view = DiagnosticsView::new(
            &DiagnosticsSnapshot::default(),
            None,
            None,
            HealthStatus::Unavailable,
        );

        assert!(
            view.rows()
                .iter()
                .take(7)
                .all(|row| row.value == UNAVAILABLE)
        );
        assert_eq!(view.rows()[7].value, "Unavailable");
    }

    #[test]
    fn rejects_non_finite_values_instead_of_rendering_them() {
        let view = DiagnosticsView::new(
            &DiagnosticsSnapshot {
                cpu_usage_percent: Some(f32::NAN),
                ..DiagnosticsSnapshot::default()
            },
            Some(Measurement {
                offset: f64::NAN,
                round_trip_delay: 0.1,
                root_distance: f64::INFINITY,
            }),
            Some(4),
            HealthStatus::Uncertain,
        );

        assert_eq!(view.rows()[2].value, UNAVAILABLE);
        assert_eq!(view.rows()[3].value, UNAVAILABLE);
        assert_eq!(view.rows()[4].value, UNAVAILABLE);
        assert_eq!(view.rows()[5].value, UNAVAILABLE);
        assert_eq!(view.rows()[6].value, "4");
        assert_eq!(view.rows()[7].value, "Uncertain");
    }
}
