//! UI-independent server failover coordination.
//!
//! The coordinator owns only candidate ordering and health state. It does not
//! perform network requests or make decisions about how a result is displayed.

use std::time::{Duration, Instant};

use crate::servers::{ServerCatalog, ServerProfile};

#[derive(Debug, Clone, Copy, Default)]
struct CandidateState {
    failures: u32,
    cooldown_until: Option<Instant>,
}

/// Selects servers in a stable order and temporarily removes failed servers.
#[derive(Debug, Clone)]
pub struct FailoverCoordinator {
    candidates: Vec<ServerProfile>,
    states: Vec<CandidateState>,
    next_index: usize,
    cooldown: Duration,
}

impl FailoverCoordinator {
    /// Creates a coordinator from an ordered list of candidate profiles.
    pub fn new(candidates: Vec<ServerProfile>, cooldown: Duration) -> Self {
        let states = vec![CandidateState::default(); candidates.len()];
        Self {
            candidates,
            states,
            next_index: 0,
            cooldown,
        }
    }

    /// Creates a coordinator using the catalog's insertion order.
    pub fn from_catalog(catalog: &ServerCatalog, cooldown: Duration) -> Self {
        Self::new(
            catalog
                .entries()
                .iter()
                .map(|entry| entry.profile().clone())
                .collect(),
            cooldown,
        )
    }

    pub fn candidates(&self) -> &[ServerProfile] {
        &self.candidates
    }

    /// Returns the next eligible candidate, starting after the last selection.
    ///
    /// A candidate is selected at most once per cursor pass. Failed candidates
    /// are skipped until their cooldown expires, which prevents an immediate
    /// retry loop even when the configured cooldown is zero.
    pub fn next_candidate(&mut self, now: Instant) -> Option<&ServerProfile> {
        self.expire_cooldowns(now);
        let len = self.candidates.len();
        if len == 0 {
            return None;
        }

        for _ in 0..len {
            let index = self.next_index;
            self.next_index = (index + 1) % len;
            if self.states[index].cooldown_until.is_none() {
                return self.candidates.get(index);
            }
        }
        None
    }

    /// Records a failed attempt and starts the candidate's cooldown.
    ///
    /// Unknown profiles are ignored. This makes it safe for callers to report
    /// a result from a worker that has already moved to another candidate.
    pub fn record_failure(&mut self, profile: &ServerProfile, now: Instant) {
        let Some(index) = self
            .candidates
            .iter()
            .position(|candidate| candidate == profile)
        else {
            return;
        };
        let state = &mut self.states[index];
        state.failures = state.failures.saturating_add(1);
        state.cooldown_until = Some(now + self.cooldown);
    }

    /// Records a successful attempt, clearing all failure and cooldown state
    /// for that candidate and making it the next candidate to consider.
    pub fn record_success(&mut self, profile: &ServerProfile) {
        let Some(index) = self
            .candidates
            .iter()
            .position(|candidate| candidate == profile)
        else {
            return;
        };
        self.states[index] = CandidateState::default();
        self.next_index = index;
    }

    pub fn failure_count(&self, profile: &ServerProfile) -> Option<u32> {
        self.candidates
            .iter()
            .position(|candidate| candidate == profile)
            .map(|index| self.states[index].failures)
    }

    pub fn is_on_cooldown(&self, profile: &ServerProfile, now: Instant) -> Option<bool> {
        self.candidates
            .iter()
            .position(|candidate| candidate == profile)
            .map(|index| {
                self.states[index]
                    .cooldown_until
                    .is_some_and(|until| until > now)
            })
    }

    fn expire_cooldowns(&mut self, now: Instant) {
        for state in &mut self.states {
            if state.cooldown_until.is_some_and(|until| until <= now) {
                state.cooldown_until = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(name: &str) -> ServerProfile {
        ServerProfile::from_strings(name, format!("{name}.example.test"), None).unwrap()
    }

    #[test]
    fn rotates_without_immediate_loops() {
        let first = profile("first");
        let second = profile("second");
        let third = profile("third");
        let mut coordinator = FailoverCoordinator::new(
            vec![first.clone(), second.clone(), third.clone()],
            Duration::from_secs(30),
        );
        let now = Instant::now();

        assert_eq!(coordinator.next_candidate(now), Some(&first));
        coordinator.record_failure(&first, now);
        assert_eq!(coordinator.next_candidate(now), Some(&second));
        coordinator.record_failure(&second, now);
        assert_eq!(coordinator.next_candidate(now), Some(&third));
    }

    #[test]
    fn success_resets_failure_state() {
        let first = profile("first");
        let second = profile("second");
        let mut coordinator =
            FailoverCoordinator::new(vec![first.clone(), second], Duration::from_secs(30));
        let now = Instant::now();

        assert_eq!(coordinator.next_candidate(now), Some(&first));
        coordinator.record_failure(&first, now);
        coordinator.record_success(&first);

        assert_eq!(coordinator.failure_count(&first), Some(0));
        assert_eq!(coordinator.next_candidate(now), Some(&first));
    }

    #[test]
    fn exhausted_candidates_return_none_until_cooldown_expires() {
        let first = profile("first");
        let second = profile("second");
        let mut coordinator =
            FailoverCoordinator::new(vec![first.clone(), second.clone()], Duration::from_secs(30));
        let now = Instant::now();

        assert_eq!(coordinator.next_candidate(now), Some(&first));
        coordinator.record_failure(&first, now);
        assert_eq!(coordinator.next_candidate(now), Some(&second));
        coordinator.record_failure(&second, now);
        assert_eq!(coordinator.next_candidate(now), None);
        assert_eq!(
            coordinator.next_candidate(now + Duration::from_secs(30)),
            Some(&first)
        );
    }
}
