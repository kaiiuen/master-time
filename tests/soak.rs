use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use master_time::measurement::MeasurementHistory;
use master_time::notifications::{Clock, NotificationCenter, NotificationKind, Severity};
use master_time::polling::{PollEvent, PollingWorker};
use master_time::recovery::{RecoveryDecision, RetryPolicy};
use master_time::servers::ServerProfile;
use master_time::transport::{NtpTransport, TransportError};

#[derive(Clone, Debug, Default)]
struct TestClock(Arc<AtomicU64>);

impl TestClock {
    fn advance(&self, amount: Duration) {
        let nanos = u64::try_from(amount.as_nanos()).expect("test duration fits in u64");
        self.0.fetch_add(nanos, Ordering::SeqCst);
    }
}

impl Clock for TestClock {
    fn now(&self) -> Duration {
        Duration::from_nanos(self.0.load(Ordering::SeqCst))
    }
}

#[test]
fn repeated_measurement_updates_keep_only_the_newest_bounded_history() {
    let mut history = MeasurementHistory::new(4);

    for sample in 0..=20 {
        history.push(sample as f64);
        assert!(history.len() <= history.capacity());
    }

    assert_eq!(
        history.samples().copied().collect::<Vec<_>>(),
        vec![17.0, 18.0, 19.0, 20.0]
    );
    assert_eq!(history.statistics().unwrap().mean, 18.5);
}

#[test]
fn notification_pruning_removes_expired_entries_and_preserves_active_entries() {
    let clock = TestClock::default();
    let mut center = NotificationCenter::new(clock.clone(), 3);

    let expiring = center
        .notify(
            "temporary",
            NotificationKind::Recovery,
            Severity::Info,
            "temporary status",
            Some(Duration::from_secs(5)),
        )
        .unwrap();
    center
        .notify(
            "persistent",
            NotificationKind::PollingError,
            Severity::Error,
            "still unavailable",
            None,
        )
        .unwrap();

    clock.advance(Duration::from_secs(4));
    assert_eq!(center.active().len(), 2);
    clock.advance(Duration::from_secs(1));
    center.prune_expired();

    assert!(!center.history().iter().any(|item| item.id() == expiring));
    assert_eq!(center.active().len(), 1);
    assert_eq!(center.top().unwrap().key(), "persistent");
}

#[test]
fn retry_recovery_cycles_reset_backoff_each_time() {
    let mut policy = RetryPolicy::new(Duration::from_millis(2), Duration::from_millis(8));

    for _cycle in 0..3 {
        assert_eq!(
            policy.on_failure(),
            RecoveryDecision::Retry {
                delay: Duration::from_millis(2),
                consecutive_failures: 1,
            }
        );
        assert_eq!(
            policy.on_failure(),
            RecoveryDecision::Retry {
                delay: Duration::from_millis(4),
                consecutive_failures: 2,
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
fn polling_shutdown_stops_after_a_bounded_failed_attempt() {
    let profile = ServerProfile::new("local test", "127.0.0.1", None).unwrap();
    let (worker, events) = PollingWorker::start_with_transport(
        profile,
        Duration::from_secs(5),
        NtpTransport::new(Duration::ZERO),
    )
    .unwrap();

    let event = events
        .recv_timeout(Duration::from_millis(250))
        .expect("the immediate failed attempt should emit an event");
    assert!(matches!(
        event,
        PollEvent::Error {
            error: master_time::ServiceError::Transport(TransportError::InvalidTimeout),
            consecutive_failures: 1,
            ..
        }
    ));

    worker.shutdown().unwrap();
    assert!(events.recv_timeout(Duration::from_millis(10)).is_err());
}
