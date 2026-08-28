//! UI-independent retry and recovery policy for polling failures.
//!
//! [`RetryPolicy`] owns only retry state. A caller supplies the result of each
//! polling attempt and uses the returned [`RecoveryDecision`] to schedule the
//! next attempt or observe that a successful attempt reset the failure streak.
//! No timers, threads, UI state, or polling implementation details are involved.

use std::time::Duration;

/// Default delay before the first retry after a polling failure.
pub const DEFAULT_INITIAL_DELAY: Duration = Duration::from_secs(1);

/// Default upper bound for retry delays.
pub const DEFAULT_MAX_DELAY: Duration = Duration::from_secs(60);

/// The result of applying one polling outcome to a retry policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryDecision {
    /// The polling attempt failed and should be retried after `delay`.
    Retry {
        /// Delay before the next polling attempt.
        delay: Duration,
        /// Number of failures in the current consecutive failure streak.
        consecutive_failures: u32,
    },
    /// The polling attempt succeeded and the failure streak was reset.
    Success {
        /// Always zero; included to make the reset explicit to callers.
        consecutive_failures: u32,
    },
}

impl RecoveryDecision {
    /// Returns the retry delay, if this decision requests a retry.
    pub const fn retry_delay(self) -> Option<Duration> {
        match self {
            Self::Retry { delay, .. } => Some(delay),
            Self::Success { .. } => None,
        }
    }

    /// Returns the failure count carried by this decision.
    pub const fn consecutive_failures(self) -> u32 {
        match self {
            Self::Retry {
                consecutive_failures,
                ..
            }
            | Self::Success {
                consecutive_failures,
            } => consecutive_failures,
        }
    }
}

/// Stateful, UI-independent exponential-backoff policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    initial_delay: Duration,
    max_delay: Duration,
    consecutive_failures: u32,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::new(DEFAULT_INITIAL_DELAY, DEFAULT_MAX_DELAY)
    }
}

impl RetryPolicy {
    /// Creates a policy with an initial delay and an inclusive maximum delay.
    ///
    /// If `max_delay` is shorter than `initial_delay`, the maximum still wins,
    /// so every returned delay remains bounded by `max_delay`.
    pub const fn new(initial_delay: Duration, max_delay: Duration) -> Self {
        Self {
            initial_delay,
            max_delay,
            consecutive_failures: 0,
        }
    }

    /// Delay used for the first failure before exponential growth is applied.
    pub const fn initial_delay(self) -> Duration {
        self.initial_delay
    }

    /// Maximum delay returned by the policy.
    pub const fn max_delay(self) -> Duration {
        self.max_delay
    }

    /// Number of failures since the last successful polling attempt.
    pub const fn consecutive_failures(self) -> u32 {
        self.consecutive_failures
    }

    /// Records a failed polling attempt and computes its bounded retry delay.
    pub fn on_failure(&mut self) -> RecoveryDecision {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        let delay = self.backoff_delay(self.consecutive_failures);

        RecoveryDecision::Retry {
            delay,
            consecutive_failures: self.consecutive_failures,
        }
    }

    /// Records a successful polling attempt and clears the failure streak.
    pub fn on_success(&mut self) -> RecoveryDecision {
        self.consecutive_failures = 0;
        RecoveryDecision::Success {
            consecutive_failures: 0,
        }
    }

    fn backoff_delay(&self, failure_number: u32) -> Duration {
        let exponent = failure_number.saturating_sub(1).min(63);
        let multiplier = 1u128 << exponent;
        let initial_nanos = self.initial_delay.as_nanos();
        let max_nanos = self.max_delay.as_nanos();
        let delay_nanos = initial_nanos
            .saturating_mul(multiplier)
            .min(max_nanos)
            .min(u64::MAX as u128);

        Duration::from_nanos(delay_nanos as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_use_exponential_backoff() {
        let mut policy = RetryPolicy::new(Duration::from_millis(250), Duration::from_secs(10));

        assert_eq!(
            policy.on_failure(),
            RecoveryDecision::Retry {
                delay: Duration::from_millis(250),
                consecutive_failures: 1,
            }
        );
        assert_eq!(
            policy.on_failure().retry_delay(),
            Some(Duration::from_millis(500))
        );
        assert_eq!(
            policy.on_failure().retry_delay(),
            Some(Duration::from_secs(1))
        );
        assert_eq!(policy.consecutive_failures(), 3);
    }

    #[test]
    fn delay_is_capped_at_maximum() {
        let mut policy = RetryPolicy::new(Duration::from_secs(3), Duration::from_secs(10));

        assert_eq!(
            policy.on_failure().retry_delay(),
            Some(Duration::from_secs(3))
        );
        assert_eq!(
            policy.on_failure().retry_delay(),
            Some(Duration::from_secs(6))
        );
        assert_eq!(
            policy.on_failure().retry_delay(),
            Some(Duration::from_secs(10))
        );
        assert_eq!(
            policy.on_failure().retry_delay(),
            Some(Duration::from_secs(10))
        );
    }

    #[test]
    fn success_resets_failures_and_backoff() {
        let mut policy = RetryPolicy::new(Duration::from_secs(1), Duration::from_secs(30));
        policy.on_failure();
        policy.on_failure();

        assert_eq!(
            policy.on_success(),
            RecoveryDecision::Success {
                consecutive_failures: 0,
            }
        );
        assert_eq!(policy.consecutive_failures(), 0);
        assert_eq!(
            policy.on_failure().retry_delay(),
            Some(Duration::from_secs(1))
        );
    }

    #[test]
    fn zero_delays_are_deterministic_and_remain_bounded() {
        let mut policy = RetryPolicy::new(Duration::ZERO, Duration::ZERO);

        for failure in 1..=4 {
            assert_eq!(
                policy.on_failure(),
                RecoveryDecision::Retry {
                    delay: Duration::ZERO,
                    consecutive_failures: failure,
                }
            );
        }
    }

    #[test]
    fn max_delay_also_bounds_an_initial_delay_that_is_too_large() {
        let mut policy = RetryPolicy::new(Duration::from_secs(10), Duration::from_secs(2));

        assert_eq!(
            policy.on_failure().retry_delay(),
            Some(Duration::from_secs(2))
        );
    }
}
