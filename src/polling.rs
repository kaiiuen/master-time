//! A stoppable background worker for recurring NTP measurements.
//!
//! The worker owns scheduling and thread control only. Measurement and health
//! policy remain in [`crate::service`], [`crate::measurement`], and
//! [`crate::health`].

use std::fmt;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::config::{self, ConfigError};
use crate::health::{self, HealthStatus, LeapIndicator};
use crate::recovery::RetryPolicy;
use crate::servers::ServerProfile;
use crate::service::{MeasurementResult, NtpMeasurementService, ServiceError};
use crate::transport::NtpTransport;

/// Events emitted after each scheduled measurement attempt.
#[derive(Debug)]
pub enum PollEvent {
    /// A measurement completed and was evaluated for synchronization health.
    Success {
        profile: ServerProfile,
        result: MeasurementResult,
        health: HealthStatus,
        /// Consecutive polling failures after this successful attempt.
        /// Always zero because success resets the retry policy.
        consecutive_failures: u32,
        /// There is no retry backoff after a successful attempt.
        retry_delay: Option<Duration>,
    },
    /// The measurement service could not produce a result.
    Error {
        profile: ServerProfile,
        error: ServiceError,
        /// Number of consecutive failures including this attempt.
        consecutive_failures: u32,
        /// Delay before the next attempt, as selected by the retry policy.
        retry_delay: Option<Duration>,
    },
}

/// Errors returned before a polling worker can be started or when it joins.
#[derive(Debug)]
pub enum PollingError {
    InvalidInterval(ConfigError),
    WorkerPanicked,
}

impl fmt::Display for PollingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInterval(source) => {
                write!(formatter, "invalid polling interval: {source}")
            }
            Self::WorkerPanicked => formatter.write_str("polling worker thread panicked"),
        }
    }
}

impl std::error::Error for PollingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidInterval(source) => Some(source),
            Self::WorkerPanicked => None,
        }
    }
}

/// Handle used to stop and join a polling worker.
pub struct PollingWorker {
    stop_sender: Option<Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl PollingWorker {
    /// Starts polling immediately, then waits `interval` between attempts.
    pub fn start(
        profile: ServerProfile,
        interval: Duration,
    ) -> Result<(Self, Receiver<PollEvent>), PollingError> {
        Self::start_with_transport(profile, interval, NtpTransport::default())
    }

    /// Starts a worker with an explicitly configured NTP transport.
    pub fn start_with_transport(
        profile: ServerProfile,
        interval: Duration,
        transport: NtpTransport,
    ) -> Result<(Self, Receiver<PollEvent>), PollingError> {
        validate_interval(interval)?;
        let service = NtpMeasurementService::new(transport);
        Ok(Self::spawn(profile, interval, move |profile| {
            service.measure(profile)
        }))
    }

    /// Requests shutdown and waits for the worker thread to finish.
    ///
    /// Shutdown is idempotent. A worker already performing a network request
    /// finishes that bounded request before joining.
    pub fn shutdown(self) -> Result<(), PollingError> {
        self.request_shutdown();
        self.join()
    }

    /// Requests shutdown without consuming the handle.
    pub fn request_shutdown(&self) {
        if let Some(sender) = &self.stop_sender {
            let _ = sender.send(());
        }
    }

    /// Waits for the worker thread after a shutdown request.
    pub fn join(mut self) -> Result<(), PollingError> {
        self.stop_sender.take();
        self.thread
            .take()
            .expect("polling worker thread already joined")
            .join()
            .map_err(|_| PollingError::WorkerPanicked)
    }

    fn spawn<F>(
        profile: ServerProfile,
        interval: Duration,
        measure: F,
    ) -> (Self, Receiver<PollEvent>)
    where
        F: Fn(&ServerProfile) -> Result<MeasurementResult, ServiceError> + Send + 'static,
    {
        Self::spawn_with_policy(profile, interval, RetryPolicy::default(), measure)
    }

    fn spawn_with_policy<F>(
        profile: ServerProfile,
        interval: Duration,
        retry_policy: RetryPolicy,
        measure: F,
    ) -> (Self, Receiver<PollEvent>)
    where
        F: Fn(&ServerProfile) -> Result<MeasurementResult, ServiceError> + Send + 'static,
    {
        let (event_sender, event_receiver) = mpsc::channel();
        let (stop_sender, stop_receiver) = mpsc::channel();
        let thread = thread::spawn(move || {
            run_loop(
                profile,
                interval,
                stop_receiver,
                event_sender,
                retry_policy,
                measure,
            );
        });

        (
            Self {
                stop_sender: Some(stop_sender),
                thread: Some(thread),
            },
            event_receiver,
        )
    }
}

impl Drop for PollingWorker {
    fn drop(&mut self) {
        self.request_shutdown();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Starts a polling worker using the default transport.
pub fn start(
    profile: ServerProfile,
    interval: Duration,
) -> Result<(PollingWorker, Receiver<PollEvent>), PollingError> {
    PollingWorker::start(profile, interval)
}

fn validate_interval(interval: Duration) -> Result<(), PollingError> {
    config::PollingPreferences::new(interval)
        .map(|_| ())
        .map_err(PollingError::InvalidInterval)
}

fn run_loop<F>(
    profile: ServerProfile,
    interval: Duration,
    stop_receiver: Receiver<()>,
    event_sender: Sender<PollEvent>,
    mut retry_policy: RetryPolicy,
    measure: F,
) where
    F: Fn(&ServerProfile) -> Result<MeasurementResult, ServiceError>,
{
    loop {
        let (event, next_delay) = match measure(&profile) {
            Ok(result) => {
                let decision = retry_policy.on_success();
                (
                    PollEvent::Success {
                        profile: profile.clone(),
                        health: evaluate_health(&result),
                        result,
                        consecutive_failures: decision.consecutive_failures(),
                        retry_delay: decision.retry_delay(),
                    },
                    interval,
                )
            }
            Err(error) => {
                let decision = retry_policy.on_failure();
                (
                    PollEvent::Error {
                        profile: profile.clone(),
                        error,
                        consecutive_failures: decision.consecutive_failures(),
                        retry_delay: decision.retry_delay(),
                    },
                    decision.retry_delay().unwrap_or(interval),
                )
            }
        };

        if event_sender.send(event).is_err() {
            break;
        }

        match stop_receiver.recv_timeout(next_delay) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

fn evaluate_health(result: &MeasurementResult) -> HealthStatus {
    let leap_indicator =
        LeapIndicator::from_bits(result.header.leap_indicator).unwrap_or(LeapIndicator::Alarm);
    health::evaluate(health::HealthInput::new(
        true,
        result.header.stratum,
        leap_indicator,
        result.measurement.root_distance,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measurement::Measurement;
    use crate::ntp::{NtpHeader, NtpTimestamp};
    use std::net::SocketAddr;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn profile() -> ServerProfile {
        ServerProfile::new("test", "example.test", None).unwrap()
    }

    fn result() -> MeasurementResult {
        MeasurementResult {
            server: "127.0.0.1:123".parse::<SocketAddr>().unwrap(),
            header: NtpHeader {
                leap_indicator: 0,
                version: 4,
                mode: 4,
                stratum: 2,
                poll_exponent: 0,
                precision_exponent: 0,
                root_delay: 0,
                root_dispersion: 0,
                reference_id: [0; 4],
                reference_timestamp: NtpTimestamp::ZERO,
                originate_timestamp: NtpTimestamp::ZERO,
                receive_timestamp: NtpTimestamp::ZERO,
                transmit_timestamp: NtpTimestamp::ZERO,
            },
            timestamps: Default::default(),
            measurement: Measurement {
                offset: 0.0,
                round_trip_delay: 0.0,
                root_distance: 0.25,
            },
        }
    }

    #[test]
    fn loop_emits_structured_success_and_stops() {
        let (worker, events) =
            PollingWorker::spawn(profile(), Duration::from_millis(1), |_| Ok(result()));
        let event = events.recv().unwrap();
        assert!(matches!(
            event,
            PollEvent::Success {
                health: HealthStatus::Synchronized,
                ..
            }
        ));
        worker.shutdown().unwrap();
    }

    #[test]
    fn loop_emits_errors_and_shutdown_prevents_next_attempt() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&attempts);
        let (worker, events) =
            PollingWorker::spawn(profile(), Duration::from_secs(60), move |_| {
                count.fetch_add(1, Ordering::SeqCst);
                Err(ServiceError::UnsupportedMode(1))
            });

        let event = events.recv().unwrap();
        assert!(matches!(
            event,
            PollEvent::Error {
                error: ServiceError::UnsupportedMode(1),
                ..
            }
        ));
        worker.shutdown().unwrap();
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn retry_metadata_reports_backoff_and_success_resets_policy() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&attempts);
        let policy = RetryPolicy::new(Duration::from_millis(1), Duration::from_millis(2));
        let (worker, events) =
            PollingWorker::spawn_with_policy(profile(), Duration::ZERO, policy, move |_| {
                let attempt = count.fetch_add(1, Ordering::SeqCst);
                match attempt {
                    0 | 1 | 3 => Err(ServiceError::UnsupportedMode(1)),
                    2 => Ok(result()),
                    _ => unreachable!("the worker should be stopped after four events"),
                }
            });

        let first = events.recv().unwrap();
        assert!(matches!(
            first,
            PollEvent::Error {
                consecutive_failures: 1,
                retry_delay: Some(delay),
                ..
            } if delay == Duration::from_millis(1)
        ));

        let second = events.recv().unwrap();
        assert!(matches!(
            second,
            PollEvent::Error {
                consecutive_failures: 2,
                retry_delay: Some(delay),
                ..
            } if delay == Duration::from_millis(2)
        ));

        let success = events.recv().unwrap();
        assert!(matches!(
            success,
            PollEvent::Success {
                consecutive_failures: 0,
                retry_delay: None,
                ..
            }
        ));

        let after_reset = events.recv().unwrap();
        assert!(matches!(
            after_reset,
            PollEvent::Error {
                consecutive_failures: 1,
                retry_delay: Some(delay),
                ..
            } if delay == Duration::from_millis(1)
        ));

        worker.shutdown().unwrap();
        assert_eq!(attempts.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn interval_is_validated_before_starting_thread() {
        let error = match PollingWorker::start(profile(), Duration::from_secs(1)) {
            Ok(_) => panic!("an interval shorter than the configured minimum must fail"),
            Err(error) => error,
        };
        assert!(matches!(error, PollingError::InvalidInterval(_)));
    }
}
