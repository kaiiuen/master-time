//! In-memory diagnostics intended for crash reports and support bundles.
//!
//! This module deliberately has no file, network, or logging dependencies. A caller can
//! copy the output returned by [`CrashDiagnostics::export_text`] without granting the
//! collector access to any external resource.

use std::collections::VecDeque;
use std::fmt::{self, Display, Write as FmtWrite};
use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

/// The importance of a diagnostic entry.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Severity {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Critical,
}

impl Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
            Self::Critical => "CRITICAL",
        };
        f.write_str(value)
    }
}

/// Supplies timestamps to a [`CrashDiagnostics`] collector.
pub trait Clock {
    fn now(&self) -> SystemTime;
}

/// The production clock used by [`CrashDiagnostics::new`].
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// One sanitized diagnostic entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticEntry {
    pub timestamp: SystemTime,
    pub severity: Severity,
    pub message: String,
}

/// A bounded, in-memory diagnostic log.
#[derive(Debug)]
pub struct CrashDiagnostics<C = SystemClock> {
    capacity: usize,
    clock: C,
    entries: VecDeque<DiagnosticEntry>,
}

impl CrashDiagnostics<SystemClock> {
    /// Creates a collector with room for at most `capacity` entries.
    ///
    /// A capacity of zero is valid and makes recording a no-op.
    pub fn new(capacity: usize) -> Self {
        Self::with_clock(capacity, SystemClock)
    }
}

impl<C: Clock> CrashDiagnostics<C> {
    /// Creates a collector using `clock`, which is useful for deterministic tests.
    pub fn with_clock(capacity: usize, clock: C) -> Self {
        Self {
            capacity,
            clock,
            entries: VecDeque::with_capacity(capacity.min(1024)),
        }
    }

    /// Records a sanitized message, dropping the oldest entry when full.
    pub fn record(&mut self, severity: Severity, message: impl AsRef<str>) {
        if self.capacity == 0 {
            return;
        }

        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(DiagnosticEntry {
            timestamp: self.clock.now(),
            severity,
            message: redact(message.as_ref()),
        });
    }

    /// Records an error without allowing error formatting or redaction to affect the caller.
    pub fn record_failure<E: Display>(&mut self, context: impl AsRef<str>, error: E) {
        let mut message = String::new();
        let _ = write!(&mut message, "{}: {}", context.as_ref(), error);
        self.record(Severity::Error, message);
    }

    /// Records success or failure and returns the original result unchanged.
    pub fn record_result<T, E: Display>(
        &mut self,
        context: impl AsRef<str>,
        result: Result<T, E>,
    ) -> Result<T, E> {
        if let Err(ref error) = result {
            self.record_failure(context, error);
        }
        result
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> impl ExactSizeIterator<Item = &DiagnosticEntry> {
        self.entries.iter()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Exports entries oldest-first. Formatting is infallible and never performs I/O.
    pub fn export_text(&self) -> String {
        let mut output = String::new();
        for entry in &self.entries {
            let _ = writeln!(
                output,
                "{} [{}] {}",
                format_timestamp(entry.timestamp),
                entry.severity,
                entry.message
            );
        }
        output
    }

    /// Writes the same representation as [`Self::export_text`].
    pub fn write_export<W: Write>(&self, mut writer: W) -> io::Result<()> {
        writer.write_all(self.export_text().as_bytes())
    }
}

fn redact(input: &str) -> String {
    // Handle the forms most often found in URLs, headers, command lines, and JSON.
    const KEYS: &[&str] = &[
        "password",
        "passwd",
        "pwd",
        "secret",
        "token",
        "api_key",
        "apikey",
        "access_key",
        "private_key",
        "authorization",
        "credential",
    ];
    let mut result = input.to_owned();
    for key in KEYS {
        for separator in ["=", ":"] {
            let mut search_from = 0;
            while let Some(relative) = result[search_from..]
                .to_ascii_lowercase()
                .find(&format!("{key}{separator}"))
            {
                let start = search_from + relative;
                let value_start = start + key.len() + separator.len();
                let value_start = value_start
                    + result[value_start..]
                        .chars()
                        .take_while(|character| character.is_whitespace())
                        .map(char::len_utf8)
                        .sum::<usize>();
                let end = sensitive_value_end(&result, value_start);
                result.replace_range(value_start..end, "[REDACTED]");
                search_from = value_start + "[REDACTED]".len();
                if search_from >= result.len() {
                    break;
                }
            }
        }
    }

    let mut output = result;
    let mut search_from = 0;
    loop {
        let lower = output.to_ascii_lowercase();
        let Some(relative) = lower[search_from..].find("bearer ") else {
            break;
        };
        let start = search_from + relative + "bearer ".len();
        let end = value_end(&output, start);
        output.replace_range(start..end, "[REDACTED]");
        search_from = start + "[REDACTED]".len();
        if search_from >= output.len() {
            break;
        }
    }
    output
}

fn sensitive_value_end(input: &str, start: usize) -> usize {
    let lower = input[start..].to_ascii_lowercase();
    if lower.starts_with("bearer ") {
        return value_end(input, start + "bearer ".len());
    }
    value_end(input, start)
}

fn value_end(input: &str, start: usize) -> usize {
    input[start..]
        .find(|character: char| {
            character.is_whitespace() || matches!(character, '&' | ',' | ';' | '}' | ']')
        })
        .map_or(input.len(), |offset| start + offset)
}

fn format_timestamp(time: SystemTime) -> String {
    let (seconds, nanos) = match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => (duration.as_secs() as i64, duration.subsec_nanos()),
        Err(error) => {
            let duration = error.duration();
            let seconds = duration.as_secs() as i64;
            if duration.subsec_nanos() == 0 {
                (-seconds, 0)
            } else {
                (-seconds - 1, 1_000_000_000 - duration.subsec_nanos())
            }
        }
    };
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:09}Z",
        day_seconds / 3_600,
        day_seconds / 60 % 60,
        day_seconds % 60,
        nanos
    )
}

// Howard Hinnant's public-domain civil calendar conversion.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let adjusted = days + 719_468;
    let era = (if adjusted >= 0 {
        adjusted
    } else {
        adjusted - 146_096
    }) / 146_097;
    let day_of_era = adjusted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    (year + i64::from(month <= 2), month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[derive(Clone, Copy)]
    struct TestClock(SystemTime);

    impl Clock for TestClock {
        fn now(&self) -> SystemTime {
            self.0
        }
    }

    #[test]
    fn keeps_latest_entries_with_a_bound() {
        let clock = TestClock(UNIX_EPOCH + Duration::from_secs(1));
        let mut diagnostics = CrashDiagnostics::with_clock(2, clock);
        diagnostics.record(Severity::Info, "first");
        diagnostics.record(Severity::Warn, "second");
        diagnostics.record(Severity::Error, "third");

        let messages: Vec<_> = diagnostics
            .entries()
            .map(|entry| entry.message.as_str())
            .collect();
        assert_eq!(messages, ["second", "third"]);
    }

    #[test]
    fn zero_capacity_and_failures_are_graceful() {
        let mut diagnostics = CrashDiagnostics::new(0);
        diagnostics.record_failure("operation", "token=do-not-leak");
        assert!(diagnostics.is_empty());
        assert_eq!(
            diagnostics.record_result::<(), &str>("operation", Err("failed")),
            Err("failed")
        );
    }

    #[test]
    fn redacts_sensitive_values_and_exports_timestamped_text() {
        let time = UNIX_EPOCH + Duration::from_secs(1_704_067_200) + Duration::from_millis(12);
        let mut diagnostics = CrashDiagnostics::with_clock(4, TestClock(time));
        diagnostics.record(
            Severity::Error,
            "https://example.test?token=abc123 authorization: Bearer secret-value user=alice",
        );
        let output = diagnostics.export_text();

        assert!(output.starts_with("2024-01-01T00:00:00.012000000Z [ERROR] "));
        assert!(output.contains("token=[REDACTED]"));
        assert!(output.contains("authorization: [REDACTED]"));
        assert!(!output.contains("abc123"));
        assert!(!output.contains("secret-value"));
        assert!(output.contains("user=alice"));
    }

    #[test]
    fn records_failures_without_changing_the_result() {
        let mut diagnostics = CrashDiagnostics::with_clock(2, TestClock(UNIX_EPOCH));
        let result = diagnostics.record_result::<(), _>("request", Err("password=hunter2"));
        assert_eq!(result, Err("password=hunter2"));
        assert_eq!(
            diagnostics.entries().next().unwrap().message,
            "request: password=[REDACTED]"
        );
    }
}
