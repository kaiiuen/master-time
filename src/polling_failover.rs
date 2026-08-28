//! Orchestration between polling events, retry state, and server failover.
//!
//! This adapter deliberately does not start or control a [`PollingWorker`]. A
//! caller owns the worker and feeds its events to [`PollingOrchestrator`]. That
//! keeps event handling deterministic and makes the failover policy testable
//! without network I/O or sleeps.

use std::time::Instant;

use crate::failover::FailoverCoordinator;
use crate::health::HealthStatus;
use crate::polling::PollEvent;
use crate::recovery::{RecoveryDecision, RetryPolicy};
use crate::servers::ServerProfile;
use crate::service::{MeasurementResult, ServiceError};

/// The state change caused by handling one polling event.
#[derive(Debug)]
pub enum PollingTransition {
    /// A measurement succeeded and both retry and candidate failure state were
    /// reset for the server.
    Success {
        profile: ServerProfile,
        result: MeasurementResult,
        health: HealthStatus,
        consecutive_failures: u32,
    },
    /// The failed server was put on cooldown and another eligible server was
    /// selected.
    Failover {
        failed_profile: ServerProfile,
        next_profile: ServerProfile,
        error: ServiceError,
        decision: RecoveryDecision,
    },
    /// Every candidate is currently on cooldown. The caller should wait and
    /// call [`PollingOrchestrator::next_server`] again when appropriate.
    Exhausted {
        failed_profile: ServerProfile,
        error: ServiceError,
        decision: RecoveryDecision,
    },
}

impl PollingTransition {
    /// Returns the profile selected by this transition, if one was selected.
    pub fn next_profile(&self) -> Option<&ServerProfile> {
        match self {
            Self::Failover { next_profile, .. } => Some(next_profile),
            Self::Success { .. } | Self::Exhausted { .. } => None,
        }
    }

    /// Returns the retry decision associated with this transition.
    pub fn retry_decision(&self) -> Option<RecoveryDecision> {
        match self {
            Self::Success {
                consecutive_failures,
                ..
            } => Some(RecoveryDecision::Success {
                consecutive_failures: *consecutive_failures,
            }),
            Self::Failover { decision, .. } | Self::Exhausted { decision, .. } => Some(*decision),
        }
    }
}

/// Consumes [`PollEvent`] values and coordinates retry state with failover.
#[derive(Debug)]
pub struct PollingOrchestrator {
    coordinator: FailoverCoordinator,
    retry_policy: RetryPolicy,
    current_profile: Option<ServerProfile>,
}

impl PollingOrchestrator {
    /// Creates an orchestrator using the supplied candidate order and policy.
    pub fn new(coordinator: FailoverCoordinator, retry_policy: RetryPolicy, now: Instant) -> Self {
        let mut coordinator = coordinator;
        let current_profile = coordinator.next_candidate(now).cloned();
        Self {
            coordinator,
            retry_policy,
            current_profile,
        }
    }

    /// Creates an orchestrator with the default retry policy.
    pub fn with_coordinator(coordinator: FailoverCoordinator, now: Instant) -> Self {
        Self::new(coordinator, RetryPolicy::default(), now)
    }

    /// Returns the server whose worker should be running, if any.
    pub fn current_profile(&self) -> Option<&ServerProfile> {
        self.current_profile.as_ref()
    }

    /// Selects the next eligible server after an exhausted candidate pass.
    pub fn next_server(&mut self, now: Instant) -> Option<ServerProfile> {
        let profile = self.coordinator.next_candidate(now).cloned();
        self.current_profile = profile.clone();
        profile
    }

    pub fn retry_policy(&self) -> RetryPolicy {
        self.retry_policy
    }

    pub fn coordinator(&self) -> &FailoverCoordinator {
        &self.coordinator
    }

    /// Applies one worker event and emits its orchestration transition.
    pub fn handle_event(&mut self, event: PollEvent, now: Instant) -> PollingTransition {
        match event {
            PollEvent::Success {
                profile,
                result,
                health,
                ..
            } => {
                let decision = self.retry_policy.on_success();
                self.coordinator.record_success(&profile);
                self.current_profile = Some(profile.clone());
                PollingTransition::Success {
                    profile,
                    result,
                    health,
                    consecutive_failures: decision.consecutive_failures(),
                }
            }
            PollEvent::Error { profile, error, .. } => {
                let decision = self.retry_policy.on_failure();
                self.coordinator.record_failure(&profile, now);
                let next_profile = self.coordinator.next_candidate(now).cloned();

                match next_profile {
                    Some(next_profile) => {
                        self.current_profile = Some(next_profile.clone());
                        PollingTransition::Failover {
                            failed_profile: profile,
                            next_profile,
                            error,
                            decision,
                        }
                    }
                    None => {
                        self.current_profile = None;
                        PollingTransition::Exhausted {
                            failed_profile: profile,
                            error,
                            decision,
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measurement::Measurement;
    use crate::ntp::{NtpHeader, NtpTimestamp};
    use std::net::SocketAddr;
    use std::time::Duration;

    fn profile(name: &str) -> ServerProfile {
        ServerProfile::from_strings(name, format!("{name}.example.test"), None).unwrap()
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

    fn error_event(profile: ServerProfile) -> PollEvent {
        PollEvent::Error {
            profile,
            error: ServiceError::UnsupportedMode(1),
            consecutive_failures: 99,
            retry_delay: Some(Duration::from_secs(99)),
        }
    }

    #[test]
    fn fake_failures_rotate_then_emit_exhaustion() {
        let first = profile("first");
        let second = profile("second");
        let now = Instant::now();
        let coordinator =
            FailoverCoordinator::new(vec![first.clone(), second.clone()], Duration::from_secs(30));
        let mut orchestrator = PollingOrchestrator::new(
            coordinator,
            RetryPolicy::new(Duration::from_millis(10), Duration::from_secs(1)),
            now,
        );

        let transition = orchestrator.handle_event(error_event(first.clone()), now);
        assert!(
            matches!(transition, PollingTransition::Failover { next_profile, decision, .. }
            if next_profile == second
                && decision.consecutive_failures() == 1
                && decision.retry_delay() == Some(Duration::from_millis(10)))
        );

        let transition = orchestrator.handle_event(error_event(second.clone()), now);
        assert!(
            matches!(transition, PollingTransition::Exhausted { decision, .. }
            if decision.consecutive_failures() == 2)
        );
        assert_eq!(orchestrator.current_profile(), None);
    }

    #[test]
    fn fake_success_resets_retry_and_candidate_state() {
        let first = profile("first");
        let second = profile("second");
        let now = Instant::now();
        let coordinator =
            FailoverCoordinator::new(vec![first.clone(), second], Duration::from_secs(30));
        let mut orchestrator = PollingOrchestrator::new(
            coordinator,
            RetryPolicy::new(Duration::from_millis(10), Duration::from_secs(1)),
            now,
        );

        let _ = orchestrator.handle_event(error_event(first.clone()), now);
        let transition = orchestrator.handle_event(
            PollEvent::Success {
                profile: first.clone(),
                result: result(),
                health: HealthStatus::Synchronized,
                consecutive_failures: 42,
                retry_delay: Some(Duration::from_secs(42)),
            },
            now,
        );

        assert!(matches!(
            transition,
            PollingTransition::Success {
                consecutive_failures: 0,
                ..
            }
        ));
        assert_eq!(orchestrator.retry_policy().consecutive_failures(), 0);
        assert_eq!(orchestrator.coordinator().failure_count(&first), Some(0));
        assert_eq!(orchestrator.current_profile(), Some(&first));
    }
}
