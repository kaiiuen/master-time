//! UI-independent presentation of rolling network statistics.
//!
//! This module intentionally contains no rendering types. A consumer can use
//! [`NetworkViewModel::rows`] to render a stable set of metric rows, or use the
//! typed values and histories for another presentation.

use crate::measurement::Statistics;
use crate::network_stats::NetworkStats;

/// Text used when a metric has no valid samples.
pub const UNAVAILABLE: &str = "unavailable";

/// Largest magnitude allowed in a display value.
///
/// This prevents malformed or unexpectedly large values from producing an
/// unbounded label in a UI. Packet loss has its own, tighter percentage bound.
pub const MAX_DISPLAY_VALUE: f64 = 1_000_000.0;

/// The fixed set of network metrics presented by this view model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkMetric {
    Offset,
    Rtt,
    Jitter,
    PacketLoss,
    FrequencyError,
}

impl NetworkMetric {
    /// Stable human-readable label for the metric.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Offset => "Offset",
            Self::Rtt => "RTT",
            Self::Jitter => "Jitter",
            Self::PacketLoss => "Packet loss",
            Self::FrequencyError => "Frequency error",
        }
    }

    /// Stable unit for the metric's numeric value.
    pub const fn unit(self) -> &'static str {
        match self {
            Self::Offset | Self::Rtt | Self::Jitter => "ms",
            Self::PacketLoss => "%",
            Self::FrequencyError => "ppm",
        }
    }

    pub const fn all() -> [Self; 5] {
        [
            Self::Offset,
            Self::Rtt,
            Self::Jitter,
            Self::PacketLoss,
            Self::FrequencyError,
        ]
    }
}

/// A metric value that can be rendered without guessing at missing data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MetricValue {
    Available(f64),
    Unavailable,
}

impl MetricValue {
    pub fn as_option(self) -> Option<f64> {
        match self {
            Self::Available(value) => Some(value),
            Self::Unavailable => None,
        }
    }

    pub fn is_available(self) -> bool {
        matches!(self, Self::Available(_))
    }
}

/// One stable, UI-independent network metric row.
#[derive(Debug, Clone, PartialEq)]
pub struct NetworkMetricRow {
    pub metric: NetworkMetric,
    pub label: &'static str,
    pub unit: &'static str,
    pub value: MetricValue,
    /// Bounded, unit-converted value suitable for display or accessibility.
    pub display_value: MetricValue,
    pub display_text: String,
    pub statistics: Option<Statistics>,
    pub history: Vec<f64>,
}

/// Stable presentation model for offset, RTT, jitter, packet loss, and
/// frequency error.
#[derive(Debug, Clone, PartialEq)]
pub struct NetworkViewModel {
    rows: Vec<NetworkMetricRow>,
}

impl NetworkViewModel {
    pub fn new(stats: &NetworkStats) -> Self {
        let rows = NetworkMetric::all()
            .into_iter()
            .map(|metric| make_row(stats, metric))
            .collect();
        Self { rows }
    }

    pub fn rows(&self) -> &[NetworkMetricRow] {
        &self.rows
    }

    pub fn row(&self, metric: NetworkMetric) -> &NetworkMetricRow {
        &self.rows[metric as usize]
    }
}

fn make_row(stats: &NetworkStats, metric: NetworkMetric) -> NetworkMetricRow {
    let (history, source_statistics) = match metric {
        NetworkMetric::Offset => (stats.offset_history(), stats.offset_statistics()),
        NetworkMetric::Rtt => (stats.rtt_history(), stats.rtt_statistics()),
        NetworkMetric::Jitter => (stats.jitter_history(), stats.jitter_statistics()),
        NetworkMetric::PacketLoss => (
            stats.packet_loss_history(),
            stats.packet_loss_history().statistics(),
        ),
        NetworkMetric::FrequencyError => (
            stats.frequency_error_history(),
            stats.frequency_error_statistics(),
        ),
    };
    let statistics = source_statistics.map(|value| scale_statistics(metric, value));
    let raw_value = statistics.and_then(|value| finite(value.mean));
    let display_value = raw_value.map(|value| bound(metric, value));
    let value = raw_value.map_or(MetricValue::Unavailable, MetricValue::Available);
    let display_value = display_value.map_or(MetricValue::Unavailable, MetricValue::Available);
    let display_text = match display_value {
        MetricValue::Available(value) => format_value(value, metric.unit()),
        MetricValue::Unavailable => UNAVAILABLE.to_owned(),
    };

    NetworkMetricRow {
        metric,
        label: metric.label(),
        unit: metric.unit(),
        value,
        display_value,
        display_text,
        statistics: statistics.filter(|value| {
            value.min.is_finite()
                && value.max.is_finite()
                && value.mean.is_finite()
                && value.standard_deviation.is_finite()
        }),
        history: history
            .samples()
            .copied()
            .filter(|value| value.is_finite())
            .collect(),
    }
}

fn finite(value: f64) -> Option<f64> {
    value.is_finite().then_some(value)
}

fn value_in_unit(metric: NetworkMetric, value: f64) -> f64 {
    match metric {
        NetworkMetric::Offset | NetworkMetric::Rtt | NetworkMetric::Jitter => value * 1_000.0,
        NetworkMetric::PacketLoss => value * 100.0,
        NetworkMetric::FrequencyError => value,
    }
}

fn scale_statistics(metric: NetworkMetric, statistics: Statistics) -> Statistics {
    Statistics {
        min: value_in_unit(metric, statistics.min),
        max: value_in_unit(metric, statistics.max),
        mean: value_in_unit(metric, statistics.mean),
        standard_deviation: value_in_unit(metric, statistics.standard_deviation),
    }
}

fn bound(metric: NetworkMetric, value: f64) -> f64 {
    let (min, max) = if metric == NetworkMetric::PacketLoss {
        (0.0, 100.0)
    } else {
        (-MAX_DISPLAY_VALUE, MAX_DISPLAY_VALUE)
    };
    value.clamp(min, max)
}

fn format_value(value: f64, unit: &str) -> String {
    format!("{value:.3} {unit}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measurement::Measurement;

    fn measurement(offset: f64, rtt: f64) -> Measurement {
        Measurement {
            offset,
            round_trip_delay: rtt,
            root_distance: 0.001,
        }
    }

    #[test]
    fn rows_have_stable_labels_units_and_order() {
        let view = NetworkViewModel::new(&NetworkStats::new(4));
        assert_eq!(
            view.rows().iter().map(|row| row.label).collect::<Vec<_>>(),
            vec!["Offset", "RTT", "Jitter", "Packet loss", "Frequency error"]
        );
        assert_eq!(
            view.rows().iter().map(|row| row.unit).collect::<Vec<_>>(),
            vec!["ms", "ms", "ms", "%", "ppm"]
        );
        assert!(view.rows().iter().all(|row| {
            row.value == MetricValue::Unavailable && row.display_text == UNAVAILABLE
        }));
    }

    #[test]
    fn presents_statistics_and_bounded_values() {
        let mut stats = NetworkStats::new(8);
        stats
            .record(Some(measurement(0.002, 0.125)), Some(3.5))
            .unwrap();
        stats.record(Some(measurement(0.004, 0.250)), None).unwrap();
        stats.record(None, None).unwrap();

        let view = NetworkViewModel::new(&stats);
        assert_eq!(
            view.row(NetworkMetric::Offset).value,
            MetricValue::Available(3.0)
        );
        assert_eq!(view.row(NetworkMetric::Offset).display_text, "3.000 ms");
        assert_eq!(view.row(NetworkMetric::Rtt).history, vec![0.125, 0.250]);
        assert_eq!(
            view.row(NetworkMetric::Jitter).value,
            MetricValue::Available(125.0)
        );
        match view.row(NetworkMetric::PacketLoss).value {
            MetricValue::Available(value) => assert!((value - 100.0 / 3.0).abs() < 1e-12),
            MetricValue::Unavailable => panic!("packet loss should be available"),
        }
        assert_eq!(
            view.row(NetworkMetric::FrequencyError).value,
            MetricValue::Available(3.5)
        );
    }

    #[test]
    fn display_values_are_capped_without_losing_raw_value() {
        let mut stats = NetworkStats::new(2);
        stats.record(Some(measurement(2_000.0, 2.0)), None).unwrap();
        let view = NetworkViewModel::new(&stats);
        assert_eq!(
            view.row(NetworkMetric::Offset).value,
            MetricValue::Available(2_000_000.0)
        );
        assert_eq!(
            view.row(NetworkMetric::Offset).display_value,
            MetricValue::Available(MAX_DISPLAY_VALUE)
        );
        assert_eq!(
            view.row(NetworkMetric::Offset).display_text,
            "1000000.000 ms"
        );
    }
}
