//! UI-independent data model for plotting measurement histories.
//!
//! The model deliberately contains no rendering types. Consumers can use the
//! normalized points with any charting or UI library they choose.

use crate::measurement::{MeasurementHistory, Statistics};

/// A point in a normalized chart series.
///
/// Both coordinates are guaranteed to be finite and in the inclusive range
/// `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NormalizedPoint {
    pub x: f64,
    pub y: f64,
}

/// The raw-value bounds represented by one chart series.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValueRange {
    pub min: f64,
    pub max: f64,
}

/// Normalized offset and round-trip-delay data ready for presentation.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartModel {
    offset_points: Vec<NormalizedPoint>,
    round_trip_points: Vec<NormalizedPoint>,
    offset_range: Option<ValueRange>,
    round_trip_range: Option<ValueRange>,
    offset_statistics: Option<Statistics>,
    round_trip_statistics: Option<Statistics>,
}

impl ChartModel {
    /// Builds a chart model from the samples in their existing order.
    ///
    /// The histories are normalized independently, so an offset and a delay
    /// with different units or magnitudes can share a chart coordinate space.
    /// Non-finite samples are omitted because they cannot be represented by a
    /// bounded chart point.
    pub fn from_histories(
        offset_history: &MeasurementHistory,
        round_trip_history: &MeasurementHistory,
    ) -> Self {
        let offset_statistics = offset_history.statistics();
        let round_trip_statistics = round_trip_history.statistics();
        let offset_range = finite_range(offset_history);
        let round_trip_range = finite_range(round_trip_history);

        Self {
            offset_points: normalized_points(offset_history, offset_range),
            round_trip_points: normalized_points(round_trip_history, round_trip_range),
            offset_range,
            round_trip_range,
            offset_statistics,
            round_trip_statistics,
        }
    }

    pub fn offset_points(&self) -> &[NormalizedPoint] {
        &self.offset_points
    }

    pub fn round_trip_points(&self) -> &[NormalizedPoint] {
        &self.round_trip_points
    }

    pub fn offset_range(&self) -> Option<ValueRange> {
        self.offset_range
    }

    pub fn round_trip_range(&self) -> Option<ValueRange> {
        self.round_trip_range
    }

    pub fn offset_statistics(&self) -> Option<Statistics> {
        self.offset_statistics
    }

    pub fn round_trip_statistics(&self) -> Option<Statistics> {
        self.round_trip_statistics
    }

    pub fn is_empty(&self) -> bool {
        self.offset_points.is_empty() && self.round_trip_points.is_empty()
    }
}

fn finite_range(history: &MeasurementHistory) -> Option<ValueRange> {
    let mut samples = history
        .samples()
        .copied()
        .filter(|sample| sample.is_finite());
    let first = samples.next()?;
    let (min, max) = samples.fold((first, first), |(min, max), sample| {
        (min.min(sample), max.max(sample))
    });
    Some(ValueRange { min, max })
}

fn normalized_points(
    history: &MeasurementHistory,
    range: Option<ValueRange>,
) -> Vec<NormalizedPoint> {
    let samples: Vec<f64> = history
        .samples()
        .copied()
        .filter(|sample| sample.is_finite())
        .collect();
    let Some(range) = range else {
        return Vec::new();
    };

    let denominator = range.max - range.min;
    let last_index = samples.len().saturating_sub(1);
    samples
        .into_iter()
        .enumerate()
        .map(|(index, sample)| NormalizedPoint {
            x: if last_index == 0 {
                0.0
            } else {
                index as f64 / last_index as f64
            },
            y: if denominator == 0.0 {
                0.5
            } else {
                ((sample - range.min) / denominator).clamp(0.0, 1.0)
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history(samples: &[f64]) -> MeasurementHistory {
        let mut history = MeasurementHistory::new(samples.len());
        for &sample in samples {
            history.push(sample);
        }
        history
    }

    #[test]
    fn empty_histories_are_safe() {
        let offset = MeasurementHistory::new(4);
        let delay = MeasurementHistory::new(4);
        let model = ChartModel::from_histories(&offset, &delay);

        assert!(model.is_empty());
        assert!(model.offset_points().is_empty());
        assert!(model.round_trip_points().is_empty());
        assert_eq!(model.offset_range(), None);
        assert_eq!(model.round_trip_range(), None);
        assert_eq!(model.offset_statistics(), None);
        assert_eq!(model.round_trip_statistics(), None);
    }

    #[test]
    fn points_are_bounded_and_constant_values_are_safe() {
        let offset = history(&[-10.0, 0.0, 10.0]);
        let delay = history(&[4.0, 4.0]);
        let model = ChartModel::from_histories(&offset, &delay);

        for point in model
            .offset_points()
            .iter()
            .chain(model.round_trip_points())
        {
            assert!(point.x.is_finite() && (0.0..=1.0).contains(&point.x));
            assert!(point.y.is_finite() && (0.0..=1.0).contains(&point.y));
        }
        assert_eq!(model.round_trip_points()[0].y, 0.5);
        assert_eq!(model.round_trip_points()[1].y, 0.5);
    }

    #[test]
    fn points_preserve_sample_order() {
        let offset = history(&[30.0, 10.0, 20.0]);
        let delay = history(&[3.0, 1.0, 2.0]);
        let model = ChartModel::from_histories(&offset, &delay);

        assert_eq!(
            model.offset_points(),
            &[
                NormalizedPoint { x: 0.0, y: 1.0 },
                NormalizedPoint { x: 0.5, y: 0.0 },
                NormalizedPoint { x: 1.0, y: 0.5 },
            ]
        );
        assert_eq!(model.round_trip_points()[0].x, 0.0);
        assert_eq!(model.round_trip_points()[2].x, 1.0);
    }

    #[test]
    fn exposes_ranges_and_current_statistics() {
        let offset = history(&[-2.0, 4.0, 8.0]);
        let delay = history(&[1.0, 3.0]);
        let model = ChartModel::from_histories(&offset, &delay);

        assert_eq!(
            model.offset_range(),
            Some(ValueRange {
                min: -2.0,
                max: 8.0
            })
        );
        assert_eq!(
            model.round_trip_range(),
            Some(ValueRange { min: 1.0, max: 3.0 })
        );
        assert_eq!(model.offset_statistics(), offset.statistics());
        assert_eq!(model.round_trip_statistics(), delay.statistics());
    }
}
