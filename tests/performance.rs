use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use master_time::history_view::ChartModel;
use master_time::measurement::MeasurementHistory;
use master_time::notifications::{Clock, NotificationCenter, NotificationKind, Severity};
use master_time::recovery::{RecoveryDecision, RetryPolicy};

#[derive(Clone)]
struct TestClock(Rc<Cell<u64>>);

impl TestClock {
    fn new() -> Self {
        Self(Rc::new(Cell::new(0)))
    }

    fn advance(&self, duration: Duration) {
        let nanos = u64::try_from(duration.as_nanos()).expect("test duration fits in u64");
        self.0.set(self.0.get() + nanos);
    }
}

impl Clock for TestClock {
    fn now(&self) -> Duration {
        Duration::from_nanos(self.0.get())
    }
}

#[test]
fn history_memory_stays_within_configured_bound() {
    const CAPACITY: usize = 16;
    const UPDATES: usize = 4096;
    let mut history = MeasurementHistory::new(CAPACITY);

    for sample in 0..UPDATES {
        history.push(sample as f64);
        assert!(history.len() <= CAPACITY);
    }

    assert_eq!(history.len(), CAPACITY);
    let samples = history.samples().copied().collect::<Vec<_>>();
    assert_eq!(samples.first().copied(), Some((UPDATES - CAPACITY) as f64));
    assert_eq!(samples.last().copied(), Some((UPDATES - 1) as f64));
}

#[test]
fn notifications_stay_within_limit_while_deduplication_preserves_capacity() {
    const CAPACITY: usize = 8;
    const NOTIFICATIONS: usize = 1024;
    let clock = TestClock::new();
    let mut center = NotificationCenter::new(clock.clone(), CAPACITY);

    for index in 0..NOTIFICATIONS {
        center.notify(
            format!("event-{index}"),
            NotificationKind::PollingError,
            Severity::Error,
            format!("failure {index}"),
            None,
        );
        assert!(center.history().len() <= CAPACITY);
    }

    assert_eq!(center.history().len(), CAPACITY);
    assert!(!center.history().iter().any(|item| item.key() == "event-0"));
    assert!(
        center
            .history()
            .iter()
            .any(|item| item.key() == "event-1023")
    );

    let id = center
        .notify(
            "event-1023",
            NotificationKind::Recovery,
            Severity::Success,
            "recovered",
            Some(Duration::from_millis(2)),
        )
        .expect("existing notification should be updated");
    assert_eq!(center.history().len(), CAPACITY);
    assert_eq!(
        center
            .history()
            .iter()
            .filter(|item| item.id() == id)
            .count(),
        1
    );
    assert_eq!(
        center
            .history()
            .iter()
            .find(|item| item.id() == id)
            .unwrap()
            .message(),
        "recovered"
    );

    clock.advance(Duration::from_millis(2));
    center.prune_expired();
    assert_eq!(center.history().len(), CAPACITY - 1);
}

#[test]
fn repeated_polling_recovery_cycles_reset_backoff_without_unbounded_state() {
    const CYCLES: usize = 256;
    let mut policy = RetryPolicy::new(Duration::from_millis(1), Duration::from_millis(4));

    for _ in 0..CYCLES {
        assert_eq!(
            policy.on_failure(),
            RecoveryDecision::Retry {
                delay: Duration::from_millis(1),
                consecutive_failures: 1,
            }
        );
        assert_eq!(
            policy.on_failure(),
            RecoveryDecision::Retry {
                delay: Duration::from_millis(2),
                consecutive_failures: 2,
            }
        );
        assert_eq!(
            policy.on_failure(),
            RecoveryDecision::Retry {
                delay: Duration::from_millis(4),
                consecutive_failures: 3,
            }
        );
        assert_eq!(
            policy.on_success(),
            RecoveryDecision::Success {
                consecutive_failures: 0,
            }
        );
        assert_eq!(policy.consecutive_failures(), 0);
    }
}

#[test]
fn chart_model_generation_is_bounded_finite_and_normalized() {
    const CAPACITY: usize = 32;
    let mut offsets = MeasurementHistory::new(CAPACITY);
    let mut delays = MeasurementHistory::new(CAPACITY);

    for index in 0..256 {
        offsets.push(index as f64 - 128.0);
        delays.push((index % 7) as f64);
    }
    offsets.push(f64::NAN);
    delays.push(f64::INFINITY);

    let model = ChartModel::from_histories(&offsets, &delays);

    assert_eq!(model.offset_points().len(), CAPACITY - 1);
    assert_eq!(model.round_trip_points().len(), CAPACITY - 1);
    assert_eq!(model.offset_range().unwrap().min, 97.0);
    assert_eq!(model.offset_range().unwrap().max, 127.0);
    assert_eq!(model.round_trip_range().unwrap().min, 0.0);
    assert_eq!(model.round_trip_range().unwrap().max, 6.0);

    for point in model
        .offset_points()
        .iter()
        .chain(model.round_trip_points())
    {
        assert!(point.x.is_finite() && (0.0..=1.0).contains(&point.x));
        assert!(point.y.is_finite() && (0.0..=1.0).contains(&point.y));
    }
}
