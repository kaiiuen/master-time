//! UI-independent notification and error-banner state.
//!
//! [`NotificationCenter`] stores recent notifications, removes expired entries,
//! and exposes the currently visible banners without making any assumptions
//! about a particular UI toolkit. Time is supplied by [`Clock`] so callers and
//! tests can use a monotonic, deterministic time source.

use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The urgency of a notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Success,
    Warning,
    Error,
    Critical,
}

/// A source/category used for the built-in notification helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationKind {
    PollingError,
    UnsafeTime,
    Recovery,
}

/// A clock used by [`NotificationCenter`].
///
/// The returned value need only be monotonic relative to other values returned
/// by the same clock. [`SystemClock`] is provided for normal application use.
pub trait Clock {
    fn now(&self) -> Duration;
}

/// A clock backed by [`SystemTime`].
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
    }
}

/// One notification retained in the bounded history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    id: u64,
    key: String,
    kind: NotificationKind,
    severity: Severity,
    message: String,
    created_at: Duration,
    expires_at: Option<Duration>,
    dismissed: bool,
}

impl Notification {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn key(&self) -> &str {
        &self.key
    }
    pub fn kind(&self) -> NotificationKind {
        self.kind
    }
    pub fn severity(&self) -> Severity {
        self.severity
    }
    pub fn message(&self) -> &str {
        &self.message
    }
    pub fn created_at(&self) -> Duration {
        self.created_at
    }
    pub fn expires_at(&self) -> Option<Duration> {
        self.expires_at
    }
    pub fn is_dismissed(&self) -> bool {
        self.dismissed
    }
    pub fn is_expired_at(&self, now: Duration) -> bool {
        self.expires_at.is_some_and(|expiry| now >= expiry)
    }
}

/// A bounded notification and error-banner store.
#[derive(Debug, Clone)]
pub struct NotificationCenter<C> {
    clock: C,
    capacity: usize,
    next_id: u64,
    history: Vec<Notification>,
}

impl<C: Clock> NotificationCenter<C> {
    /// Creates a store. A capacity of zero retains no history.
    pub fn new(clock: C, capacity: usize) -> Self {
        Self {
            clock,
            capacity,
            next_id: 1,
            history: Vec::new(),
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
    pub fn now(&self) -> Duration {
        self.clock.now()
    }
    pub fn history(&self) -> &[Notification] {
        &self.history
    }

    /// Removes expired entries and returns all active, non-dismissed banners.
    pub fn active(&mut self) -> Vec<&Notification> {
        self.prune_expired();
        self.history.iter().filter(|item| !item.dismissed).collect()
    }

    /// Removes expired entries and returns the highest-severity active banner.
    pub fn top(&mut self) -> Option<&Notification> {
        self.prune_expired();
        self.history
            .iter()
            .filter(|item| !item.dismissed)
            .max_by_key(|item| (item.severity, item.id))
    }

    /// Publishes a notification, deduplicating an existing active entry by key.
    /// Deduplication updates its text, severity, and expiry rather than adding
    /// another banner, and makes a dismissed entry visible again.
    pub fn notify(
        &mut self,
        key: impl Into<String>,
        kind: NotificationKind,
        severity: Severity,
        message: impl Into<String>,
        lifetime: Option<Duration>,
    ) -> Option<u64> {
        let now = self.clock.now();
        self.prune_expired_at(now);
        let key = key.into();
        let message = message.into();
        let expires_at = lifetime.and_then(|duration| now.checked_add(duration));

        if let Some(existing) = self.history.iter_mut().find(|item| item.key == key) {
            existing.kind = kind;
            existing.severity = severity;
            existing.message = message;
            existing.created_at = now;
            existing.expires_at = expires_at;
            existing.dismissed = false;
            return Some(existing.id);
        }

        if self.capacity == 0 {
            return None;
        }
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.history.push(Notification {
            id,
            key,
            kind,
            severity,
            message,
            created_at: now,
            expires_at,
            dismissed: false,
        });
        while self.history.len() > self.capacity {
            self.history.remove(0);
        }
        Some(id)
    }

    /// Records a polling failure as an error banner.
    pub fn polling_error(&mut self, message: impl Into<String>) -> Option<u64> {
        self.notify(
            "polling-error",
            NotificationKind::PollingError,
            Severity::Error,
            message,
            None,
        )
    }

    /// Records an unsafe time status as a warning banner.
    pub fn unsafe_time(&mut self, message: impl Into<String>) -> Option<u64> {
        self.notify(
            "unsafe-time",
            NotificationKind::UnsafeTime,
            Severity::Warning,
            message,
            None,
        )
    }

    /// Records successful recovery from a prior problem.
    pub fn recovery(&mut self, message: impl Into<String>) -> Option<u64> {
        self.notify(
            "recovery",
            NotificationKind::Recovery,
            Severity::Success,
            message,
            Some(Duration::from_secs(5)),
        )
    }

    /// Dismisses an active banner. Returns whether the id was found and active.
    pub fn dismiss(&mut self, id: u64) -> bool {
        self.prune_expired();
        if let Some(item) = self
            .history
            .iter_mut()
            .find(|item| item.id == id && !item.dismissed)
        {
            item.dismissed = true;
            true
        } else {
            false
        }
    }

    /// Deletes expired entries and keeps the history bounded.
    pub fn prune_expired(&mut self) {
        self.prune_expired_at(self.clock.now());
    }

    fn prune_expired_at(&mut self, now: Duration) {
        self.history.retain(|item| !item.is_expired_at(now));
    }
}

impl Default for NotificationCenter<SystemClock> {
    fn default() -> Self {
        Self::new(SystemClock, 32)
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Critical => "critical",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[derive(Debug, Default)]
    struct TestClock(Cell<Duration>);
    impl TestClock {
        fn advance(&self, amount: Duration) {
            self.0.set(self.0.get() + amount);
        }
    }
    impl Clock for TestClock {
        fn now(&self) -> Duration {
            self.0.get()
        }
    }

    #[test]
    fn deduplicates_and_refreshes_a_banner() {
        let clock = TestClock::default();
        let mut center = NotificationCenter::new(clock, 4);
        let first = center
            .notify(
                "network",
                NotificationKind::PollingError,
                Severity::Error,
                "offline",
                Some(Duration::from_secs(5)),
            )
            .unwrap();
        center.clock.advance(Duration::from_secs(4));
        let second = center
            .notify(
                "network",
                NotificationKind::PollingError,
                Severity::Critical,
                "still offline",
                Some(Duration::from_secs(5)),
            )
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(center.history().len(), 1);
        assert_eq!(center.history()[0].message(), "still offline");
        center.clock.advance(Duration::from_secs(4));
        assert_eq!(center.active().len(), 1);
        center.clock.advance(Duration::from_secs(1));
        assert!(center.active().is_empty());
    }

    #[test]
    fn dismissal_hides_until_a_deduplicated_update() {
        let mut center = NotificationCenter::new(TestClock::default(), 4);
        let id = center.polling_error("offline").unwrap();
        assert!(center.dismiss(id));
        assert!(center.active().is_empty());
        assert!(!center.dismiss(id));
        assert_eq!(center.polling_error("again"), Some(id));
        assert_eq!(center.active()[0].message(), "again");
    }

    #[test]
    fn history_is_bounded_and_top_prefers_severity() {
        let mut center = NotificationCenter::new(TestClock::default(), 2);
        center.notify(
            "one",
            NotificationKind::Recovery,
            Severity::Info,
            "one",
            None,
        );
        center.notify(
            "two",
            NotificationKind::UnsafeTime,
            Severity::Warning,
            "two",
            None,
        );
        center.notify(
            "three",
            NotificationKind::PollingError,
            Severity::Critical,
            "three",
            None,
        );
        assert_eq!(center.history().len(), 2);
        assert_eq!(center.top().unwrap().message(), "three");
        assert!(center.history().iter().all(|item| item.message() != "one"));
    }

    #[test]
    fn built_in_events_have_expected_kinds_and_recovery_expires() {
        let clock = TestClock::default();
        let mut center = NotificationCenter::new(clock, 8);
        assert_eq!(center.unsafe_time("clock check failed").unwrap(), 1);
        assert_eq!(center.recovery("clock is safe").unwrap(), 2);
        assert_eq!(center.history()[1].kind(), NotificationKind::Recovery);
        center.clock.advance(Duration::from_secs(5));
        center.prune_expired();
        assert_eq!(center.history().len(), 1);
        assert_eq!(center.history()[0].kind(), NotificationKind::UnsafeTime);
    }
}
