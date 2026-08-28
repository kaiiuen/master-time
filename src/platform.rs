//! Cross-platform diagnostics collection.
//!
//! The public API deliberately represents platform support with `Option` values:
//! `None` means that a diagnostic is unavailable on the current platform (or
//! could not be read), rather than inventing a value for it.

use std::fmt;
use std::time::Duration;

/// A point-in-time view of the diagnostics supported by this crate.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DiagnosticsSnapshot {
    /// Time since the operating system was started.
    pub uptime: Option<Duration>,
    /// Number of logical processors visible to the operating system.
    pub logical_cpu_count: Option<usize>,
    /// Approximate total CPU utilization at collection time, in percent.
    pub cpu_usage_percent: Option<f32>,
}

impl fmt::Display for DiagnosticsSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "uptime={}, logical CPUs={}, CPU usage={}",
            format_duration(self.uptime),
            format_option(self.logical_cpu_count, |count| count.to_string()),
            format_option(self.cpu_usage_percent, |usage| format!("{usage:.1}%")),
        )
    }
}

/// Collects platform diagnostics.
///
/// On Windows, the collector keeps one CPU-times sample so subsequent calls
/// can calculate utilization. The first call reports CPU utilization as
/// unavailable because there is no earlier sample to compare with.
#[derive(Debug, Default)]
pub struct DiagnosticsCollector {
    #[cfg(windows)]
    previous_cpu_times: Option<CpuTimes>,
}

impl DiagnosticsCollector {
    /// Creates a collector with no prior CPU sample.
    pub const fn new() -> Self {
        Self {
            #[cfg(windows)]
            previous_cpu_times: None,
        }
    }

    /// Collects a new diagnostics snapshot.
    pub fn collect(&mut self) -> DiagnosticsSnapshot {
        collect_snapshot(self)
    }
}

/// Collects a diagnostics snapshot without retaining caller-visible state.
///
/// CPU utilization is unavailable because a second sample is required. Use
/// [`DiagnosticsCollector`] when CPU utilization is needed.
pub fn collect_diagnostics() -> DiagnosticsSnapshot {
    DiagnosticsCollector::new().collect()
}

fn format_duration(duration: Option<Duration>) -> String {
    match duration {
        Some(duration) => {
            let seconds = duration.as_secs();
            format!(
                "{}d {:02}h {:02}m {:02}s",
                seconds / 86_400,
                (seconds / 3_600) % 24,
                (seconds / 60) % 60,
                seconds % 60
            )
        }
        None => "unavailable".to_owned(),
    }
}

fn format_option<T>(value: Option<T>, format: impl FnOnce(T) -> String) -> String {
    value.map_or_else(|| "unavailable".to_owned(), format)
}

#[cfg(windows)]
fn collect_snapshot(collector: &mut DiagnosticsCollector) -> DiagnosticsSnapshot {
    let uptime = Some(Duration::from_millis(windows::get_tick_count64()));
    let logical_cpu_count = std::thread::available_parallelism()
        .ok()
        .map(|count| count.get());
    let cpu_usage_percent = windows::cpu_usage_percent(&mut collector.previous_cpu_times);

    DiagnosticsSnapshot {
        uptime,
        logical_cpu_count,
        cpu_usage_percent,
    }
}

#[cfg(not(windows))]
fn collect_snapshot(_collector: &mut DiagnosticsCollector) -> DiagnosticsSnapshot {
    DiagnosticsSnapshot::default()
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug)]
struct CpuTimes {
    idle: u64,
    kernel: u64,
    user: u64,
}

#[cfg(windows)]
mod windows {
    use super::CpuTimes;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "GetTickCount64"]
        fn get_tick_count64_sys() -> u64;
        fn GetSystemTimes(
            idle_time: *mut FileTime,
            kernel_time: *mut FileTime,
            user_time: *mut FileTime,
        ) -> i32;
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    impl FileTime {
        fn ticks(self) -> u64 {
            (u64::from(self.high) << 32) | u64::from(self.low)
        }
    }

    pub(super) fn get_tick_count64() -> u64 {
        // SAFETY: GetTickCount64 has no arguments and is available on supported Windows versions.
        unsafe { get_tick_count64_sys() }
    }

    pub(super) fn cpu_usage_percent(previous: &mut Option<CpuTimes>) -> Option<f32> {
        let mut idle = FileTime::default();
        let mut kernel = FileTime::default();
        let mut user = FileTime::default();

        // SAFETY: The pointers refer to valid writable FileTime values for the duration of the call.
        let success = unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } != 0;
        if !success {
            return None;
        }

        let current = CpuTimes {
            idle: idle.ticks(),
            kernel: kernel.ticks(),
            user: user.ticks(),
        };
        let result = previous.replace(current).and_then(|old| {
            let idle_delta = current.idle.checked_sub(old.idle)?;
            let total_delta = current
                .kernel
                .checked_sub(old.kernel)?
                .checked_add(current.user.checked_sub(old.user)?)?;
            if total_delta == 0 {
                None
            } else {
                Some((1.0 - (idle_delta as f32 / total_delta as f32)).clamp(0.0, 1.0) * 100.0)
            }
        });

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(windows))]
    #[test]
    fn diagnostics_are_explicitly_unavailable_off_windows() {
        let snapshot = collect_diagnostics();

        assert_eq!(snapshot, DiagnosticsSnapshot::default());
        assert_eq!(
            snapshot.to_string(),
            "uptime=unavailable, logical CPUs=unavailable, CPU usage=unavailable"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn collector_stays_unavailable_off_windows() {
        let mut collector = DiagnosticsCollector::new();

        assert_eq!(collector.collect(), DiagnosticsSnapshot::default());
        assert_eq!(collector.collect(), DiagnosticsSnapshot::default());
    }

    #[test]
    fn snapshot_formats_a_duration() {
        let snapshot = DiagnosticsSnapshot {
            uptime: Some(Duration::from_secs(90_061)),
            logical_cpu_count: Some(8),
            cpu_usage_percent: Some(12.34),
        };

        assert_eq!(
            snapshot.to_string(),
            "uptime=1d 01h 01m 01s, logical CPUs=8, CPU usage=12.3%"
        );
    }
}
