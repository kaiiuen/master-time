//! UI-independent rolling network statistics.
//!
//! [`NetworkStatistics`] records packet outcomes and optional measurements. It
//! keeps each metric in a bounded [`MeasurementHistory`], so callers can use
//! the same statistics and history concepts as the measurement layer without
//! depending on a UI or a particular presentation format.

use crate::measurement::{Measurement, MeasurementHistory, Statistics};

/// An invalid floating-point value supplied to the accumulator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkStatisticsError {
    /// A measurement field or frequency error was not finite.
    NonFiniteValue,
}

/// Bounded rolling statistics for network synchronization quality.
///
/// Every call to [`Self::record`] represents one packet attempt. A missing
/// measurement represents a lost packet and contributes to packet loss, but
/// contributes no value to the other metric histories. Jitter is the absolute
/// difference between successive valid RTT samples, and frequency error is an
/// optional caller-provided sample in the caller's chosen units.
#[derive(Debug, Clone, PartialEq)]
pub struct NetworkStatistics {
    offset: MeasurementHistory,
    round_trip_time: MeasurementHistory,
    jitter: MeasurementHistory,
    packet_loss: MeasurementHistory,
    frequency_error: MeasurementHistory,
    previous_round_trip_time: Option<f64>,
}

/// Short, explicit name for callers that prefer the accumulator terminology.
pub type NetworkStatisticsAccumulator = NetworkStatistics;
/// Conventional short name for the statistics accumulator.
pub type NetworkStats = NetworkStatistics;

impl NetworkStatistics {
    /// Creates an accumulator retaining at most `capacity` samples per metric.
    pub fn new(capacity: usize) -> Self {
        Self {
            offset: MeasurementHistory::new(capacity),
            round_trip_time: MeasurementHistory::new(capacity),
            jitter: MeasurementHistory::new(capacity),
            packet_loss: MeasurementHistory::new(capacity),
            frequency_error: MeasurementHistory::new(capacity),
            previous_round_trip_time: None,
        }
    }

    /// Records one packet attempt.
    ///
    /// `None` records a lost packet. For a successful packet, all measurement
    /// fields must be finite. A missing frequency error is simply omitted.
    /// Validation occurs before any history is changed.
    pub fn record(
        &mut self,
        measurement: Option<Measurement>,
        frequency_error: Option<f64>,
    ) -> Result<(), NetworkStatisticsError> {
        if let Some(value) = frequency_error {
            ensure_finite(value)?;
        }
        if let Some(value) = measurement {
            ensure_measurement_is_finite(value)?;
        }

        self.packet_loss
            .push(if measurement.is_some() { 0.0 } else { 1.0 });
        let Some(measurement) = measurement else {
            self.previous_round_trip_time = None;
            return Ok(());
        };

        self.offset.push(measurement.offset);
        self.round_trip_time.push(measurement.round_trip_delay);
        if let Some(previous) = self.previous_round_trip_time {
            self.jitter
                .push((measurement.round_trip_delay - previous).abs());
        }
        self.previous_round_trip_time = Some(measurement.round_trip_delay);
        if let Some(value) = frequency_error {
            self.frequency_error.push(value);
        }
        Ok(())
    }

    /// Clears all samples and the RTT state used to calculate the next jitter.
    pub fn reset(&mut self) {
        let capacity = self.capacity();
        *self = Self::new(capacity);
    }

    pub fn capacity(&self) -> usize {
        self.offset.capacity()
    }

    pub fn offset_history(&self) -> &MeasurementHistory {
        &self.offset
    }

    pub fn round_trip_time_history(&self) -> &MeasurementHistory {
        &self.round_trip_time
    }

    /// Alias for [`Self::round_trip_time_history`].
    pub fn rtt_history(&self) -> &MeasurementHistory {
        self.round_trip_time_history()
    }

    pub fn jitter_history(&self) -> &MeasurementHistory {
        &self.jitter
    }

    pub fn packet_loss_history(&self) -> &MeasurementHistory {
        &self.packet_loss
    }

    pub fn frequency_error_history(&self) -> &MeasurementHistory {
        &self.frequency_error
    }

    pub fn offset_statistics(&self) -> Option<Statistics> {
        self.offset.statistics()
    }

    pub fn round_trip_time_statistics(&self) -> Option<Statistics> {
        self.round_trip_time.statistics()
    }

    /// Alias for [`Self::round_trip_time_statistics`].
    pub fn rtt_statistics(&self) -> Option<Statistics> {
        self.round_trip_time_statistics()
    }

    pub fn jitter_statistics(&self) -> Option<Statistics> {
        self.jitter.statistics()
    }

    pub fn frequency_error_statistics(&self) -> Option<Statistics> {
        self.frequency_error.statistics()
    }

    /// Returns packet loss as a ratio in the inclusive range `0.0..=1.0`.
    pub fn packet_loss(&self) -> Option<f64> {
        self.packet_loss.statistics().map(|stats| stats.mean)
    }

    /// Returns packet loss as a percentage in the inclusive range `0.0..=100.0`.
    pub fn packet_loss_percent(&self) -> Option<f64> {
        self.packet_loss().map(|ratio| ratio * 100.0)
    }
}

impl Default for NetworkStatistics {
    fn default() -> Self {
        Self::new(120)
    }
}

fn ensure_finite(value: f64) -> Result<(), NetworkStatisticsError> {
    value
        .is_finite()
        .then_some(())
        .ok_or(NetworkStatisticsError::NonFiniteValue)
}

fn ensure_measurement_is_finite(measurement: Measurement) -> Result<(), NetworkStatisticsError> {
    ensure_finite(measurement.offset)?;
    ensure_finite(measurement.round_trip_delay)?;
    ensure_finite(measurement.root_distance)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measurement(offset: f64, round_trip_delay: f64) -> Measurement {
        Measurement {
            offset,
            round_trip_delay,
            root_distance: 0.01,
        }
    }

    fn samples(history: &MeasurementHistory) -> Vec<f64> {
        history.samples().copied().collect()
    }

    #[test]
    fn records_metrics_and_calculates_jitter_deterministically() {
        let mut stats = NetworkStatistics::new(4);
        stats
            .record(Some(measurement(1.0, 10.0)), Some(2.0))
            .unwrap();
        stats
            .record(Some(measurement(3.0, 14.0)), Some(4.0))
            .unwrap();
        stats.record(Some(measurement(5.0, 11.0)), None).unwrap();

        assert_eq!(samples(stats.offset_history()), vec![1.0, 3.0, 5.0]);
        assert_eq!(
            samples(stats.round_trip_time_history()),
            vec![10.0, 14.0, 11.0]
        );
        assert_eq!(samples(stats.jitter_history()), vec![4.0, 3.0]);
        assert_eq!(samples(stats.frequency_error_history()), vec![2.0, 4.0]);
        assert_eq!(samples(stats.packet_loss_history()), vec![0.0, 0.0, 0.0]);
        assert_eq!(stats.packet_loss(), Some(0.0));
    }

    #[test]
    fn loss_omits_missing_values_and_breaks_jitter_chain() {
        let mut stats = NetworkStatistics::new(4);
        stats.record(Some(measurement(1.0, 10.0)), None).unwrap();
        stats.record(None, None).unwrap();
        stats.record(Some(measurement(2.0, 20.0)), None).unwrap();

        assert_eq!(samples(stats.offset_history()), vec![1.0, 2.0]);
        assert_eq!(samples(stats.round_trip_time_history()), vec![10.0, 20.0]);
        assert!(stats.jitter_history().is_empty());
        assert_eq!(stats.packet_loss(), Some(1.0 / 3.0));
        assert!((stats.packet_loss_percent().unwrap() - 100.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn histories_are_bounded_and_reset_preserves_capacity() {
        let mut stats = NetworkStatistics::new(2);
        stats.record(Some(measurement(1.0, 10.0)), None).unwrap();
        stats.record(Some(measurement(2.0, 20.0)), None).unwrap();
        stats.record(Some(measurement(3.0, 30.0)), None).unwrap();
        assert_eq!(samples(stats.offset_history()), vec![2.0, 3.0]);
        assert_eq!(samples(stats.jitter_history()), vec![10.0, 10.0]);

        stats.reset();
        assert_eq!(stats.capacity(), 2);
        assert!(stats.offset_history().is_empty());
        assert!(stats.jitter_history().is_empty());
        assert!(stats.packet_loss().is_none());
    }

    #[test]
    fn rejects_non_finite_values_without_mutating_state() {
        let mut stats = NetworkStatistics::new(3);
        stats.record(Some(measurement(1.0, 2.0)), None).unwrap();
        let before = stats.clone();

        assert_eq!(
            stats.record(Some(measurement(f64::NAN, 2.0)), None),
            Err(NetworkStatisticsError::NonFiniteValue)
        );
        assert_eq!(stats, before);
        assert_eq!(
            stats.record(Some(measurement(1.0, 2.0)), Some(f64::INFINITY)),
            Err(NetworkStatisticsError::NonFiniteValue)
        );
        assert_eq!(stats, before);
    }

    #[test]
    fn zero_capacity_is_safe() {
        let mut stats = NetworkStatistics::new(0);
        stats.record(Some(measurement(1.0, 2.0)), None).unwrap();
        assert!(stats.offset_statistics().is_none());
        assert!(stats.packet_loss().is_none());
    }
}
