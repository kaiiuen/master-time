//! Standalone NTP four-timestamp measurements and rolling statistics.
//!
//! This module intentionally has no dependency on the packet or networking
//! layers. Timestamps are represented as NTP seconds (seconds since 1900,
//! plus a 32-bit fractional part), while calculated values are in seconds.

use std::collections::VecDeque;
use std::fmt;

/// An NTP timestamp: whole seconds and a binary fraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NtpTimestamp {
    pub seconds: u32,
    pub fraction: u32,
}

impl NtpTimestamp {
    pub const ZERO: Self = Self {
        seconds: 0,
        fraction: 0,
    };

    pub const fn new(seconds: u32, fraction: u32) -> Self {
        Self { seconds, fraction }
    }

    /// Converts this timestamp to fractional NTP seconds.
    pub fn as_seconds(self) -> f64 {
        self.seconds as f64 + self.fraction as f64 / 4_294_967_296.0
    }
}

/// The four timestamps captured during one NTP exchange.
///
/// T1 and T4 are local send and receive times; T2 and T3 are server receive
/// and send times. A response with any missing timestamp cannot be measured.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FourTimestamps {
    pub originate: Option<NtpTimestamp>,
    pub receive: Option<NtpTimestamp>,
    pub transmit: Option<NtpTimestamp>,
    pub destination: Option<NtpTimestamp>,
}

impl FourTimestamps {
    pub const fn new(
        originate: Option<NtpTimestamp>,
        receive: Option<NtpTimestamp>,
        transmit: Option<NtpTimestamp>,
        destination: Option<NtpTimestamp>,
    ) -> Self {
        Self {
            originate,
            receive,
            transmit,
            destination,
        }
    }

    pub const fn complete(
        originate: NtpTimestamp,
        receive: NtpTimestamp,
        transmit: NtpTimestamp,
        destination: NtpTimestamp,
    ) -> Self {
        Self::new(
            Some(originate),
            Some(receive),
            Some(transmit),
            Some(destination),
        )
    }
}

/// A measurement derived from one complete NTP exchange, in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measurement {
    pub offset: f64,
    pub round_trip_delay: f64,
    pub root_distance: f64,
}

/// Which timestamp was absent from an exchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimestampName {
    Originate,
    Receive,
    Transmit,
    Destination,
}

impl fmt::Display for TimestampName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Originate => "originate (T1)",
            Self::Receive => "receive (T2)",
            Self::Transmit => "transmit (T3)",
            Self::Destination => "destination (T4)",
        })
    }
}

/// Errors produced when an NTP exchange cannot be measured.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MeasurementError {
    MissingTimestamp(TimestampName),
    InvalidRootDistanceInput,
    NegativeRoundTripDelay,
}

impl fmt::Display for MeasurementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingTimestamp(name) => write!(f, "missing {name} timestamp"),
            Self::InvalidRootDistanceInput => {
                f.write_str("root delay and dispersion must be finite and non-negative")
            }
            Self::NegativeRoundTripDelay => f.write_str("calculated round-trip delay is negative"),
        }
    }
}

impl std::error::Error for MeasurementError {}

/// Calculates offset, network round-trip delay, and NTP root distance.
///
/// `root_delay` and `root_dispersion` are the server's NTP header values, in
/// seconds. Root distance is `root_delay / 2 + root_dispersion`.
pub fn calculate(
    timestamps: FourTimestamps,
    root_delay: f64,
    root_dispersion: f64,
) -> Result<Measurement, MeasurementError> {
    let t1 = timestamps
        .originate
        .ok_or(MeasurementError::MissingTimestamp(TimestampName::Originate))?
        .as_seconds();
    let t2 = timestamps
        .receive
        .ok_or(MeasurementError::MissingTimestamp(TimestampName::Receive))?
        .as_seconds();
    let t3 = timestamps
        .transmit
        .ok_or(MeasurementError::MissingTimestamp(TimestampName::Transmit))?
        .as_seconds();
    let t4 = timestamps
        .destination
        .ok_or(MeasurementError::MissingTimestamp(
            TimestampName::Destination,
        ))?
        .as_seconds();

    if !root_delay.is_finite()
        || root_delay < 0.0
        || !root_dispersion.is_finite()
        || root_dispersion < 0.0
    {
        return Err(MeasurementError::InvalidRootDistanceInput);
    }

    let round_trip_delay = (t4 - t1) - (t3 - t2);
    if !round_trip_delay.is_finite() || round_trip_delay < 0.0 {
        return Err(MeasurementError::NegativeRoundTripDelay);
    }

    Ok(Measurement {
        offset: ((t2 - t1) + (t3 - t4)) / 2.0,
        round_trip_delay,
        root_distance: root_delay / 2.0 + root_dispersion,
    })
}

/// Summary statistics for the samples currently in a rolling history.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Statistics {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    /// Population standard deviation (the divisor is the number of samples).
    pub standard_deviation: f64,
}

/// A bounded, newest-first rolling history of scalar measurement samples.
#[derive(Debug, Clone, PartialEq)]
pub struct MeasurementHistory {
    samples: VecDeque<f64>,
    capacity: usize,
}

impl MeasurementHistory {
    /// Creates a history. A zero capacity is allowed and retains no samples.
    pub fn new(capacity: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Adds a sample, evicting the oldest sample when at capacity.
    pub fn push(&mut self, sample: f64) {
        if self.capacity == 0 {
            return;
        }
        if self.samples.len() == self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    pub fn samples(&self) -> impl ExactSizeIterator<Item = &f64> {
        self.samples.iter()
    }

    pub fn statistics(&self) -> Option<Statistics> {
        let mut iter = self.samples.iter().copied();
        let first = iter.next()?;
        let mut min = first;
        let mut max = first;
        let mut sum = first;
        let mut count = 1usize;

        for sample in iter {
            min = min.min(sample);
            max = max.max(sample);
            sum += sample;
            count += 1;
        }

        let mean = sum / count as f64;
        let variance = self
            .samples
            .iter()
            .map(|sample| {
                let difference = *sample - mean;
                difference * difference
            })
            .sum::<f64>()
            / count as f64;

        Some(Statistics {
            min,
            max,
            mean,
            standard_deviation: variance.sqrt(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timestamp(seconds: u32) -> NtpTimestamp {
        NtpTimestamp::new(seconds, 0)
    }

    #[test]
    fn calculates_ntp_four_timestamp_formulas() {
        let exchange = FourTimestamps::complete(
            timestamp(1_000),
            timestamp(1_005),
            timestamp(1_007),
            timestamp(1_014),
        );
        let measurement = calculate(exchange, 0.004, 0.003).unwrap();

        assert!((measurement.offset + 1.0).abs() < f64::EPSILON);
        assert!((measurement.round_trip_delay - 12.0).abs() < f64::EPSILON);
        assert!((measurement.root_distance - 0.005).abs() < f64::EPSILON);
    }

    #[test]
    fn reports_missing_timestamps() {
        let exchange = FourTimestamps::new(
            None,
            Some(timestamp(2)),
            Some(timestamp(3)),
            Some(timestamp(4)),
        );
        assert_eq!(
            calculate(exchange, 0.0, 0.0),
            Err(MeasurementError::MissingTimestamp(TimestampName::Originate))
        );
    }

    #[test]
    fn rejects_invalid_inputs() {
        let exchange =
            FourTimestamps::complete(timestamp(10), timestamp(11), timestamp(12), timestamp(13));
        assert_eq!(
            calculate(exchange, -1.0, 0.0),
            Err(MeasurementError::InvalidRootDistanceInput)
        );

        let backwards =
            FourTimestamps::complete(timestamp(10), timestamp(12), timestamp(15), timestamp(12));
        assert_eq!(
            calculate(backwards, 0.0, 0.0),
            Err(MeasurementError::NegativeRoundTripDelay)
        );
    }

    #[test]
    fn rolling_history_evicts_oldest_and_calculates_statistics() {
        let mut history = MeasurementHistory::new(3);
        history.push(1.0);
        history.push(2.0);
        history.push(3.0);
        history.push(4.0);

        assert_eq!(history.len(), 3);
        assert_eq!(
            history.samples().copied().collect::<Vec<_>>(),
            vec![2.0, 3.0, 4.0]
        );
        let stats = history.statistics().unwrap();
        assert_eq!(stats.min, 2.0);
        assert_eq!(stats.max, 4.0);
        assert_eq!(stats.mean, 3.0);
        assert!((stats.standard_deviation - (2.0f64 / 3.0).sqrt()).abs() < 1e-12);
    }

    #[test]
    fn empty_and_zero_capacity_histories_have_no_statistics() {
        assert!(MeasurementHistory::new(2).statistics().is_none());
        let mut history = MeasurementHistory::new(0);
        history.push(1.0);
        assert!(history.is_empty());
        assert!(history.statistics().is_none());
    }
}
