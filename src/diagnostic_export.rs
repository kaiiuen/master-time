//! Stable exports for the application's current diagnostic state.
//!
//! The output deliberately uses only the standard library. Field names and
//! ordering are part of the format contract; consumers should treat them as
//! stable across releases.

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

use crate::measurement::{Measurement, Statistics};
use crate::network_stats::NetworkStatistics;
use crate::{DiagnosticsSnapshot, HealthStatus};

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The current state included in a diagnostic export.
#[derive(Debug, Clone, Default)]
pub struct DiagnosticSnapshot {
    pub diagnostics: DiagnosticsSnapshot,
    pub health: Option<HealthStatus>,
    pub measurement: Option<Measurement>,
    pub server: Option<ServerInfo>,
    pub network: Option<NetworkStatistics>,
}

/// The server identity shown in an export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInfo {
    pub name: String,
    pub address: String,
}

impl ServerInfo {
    pub fn new(name: impl Into<String>, address: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            address: address.into(),
        }
    }
}

/// Errors returned while rendering or atomically writing an export.
#[derive(Debug)]
pub enum ExportError {
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for ExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io {
                operation,
                path,
                source,
            } => {
                write!(
                    formatter,
                    "failed to {operation} '{}': {source}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for ExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
        }
    }
}

impl DiagnosticSnapshot {
    /// Renders a deterministic, human-readable plain-text export.
    pub fn to_plain_text(&self) -> String {
        let mut output = String::from("master-time diagnostics\nversion=1\n\n[diagnostics]\n");
        let diagnostics = &self.diagnostics;
        line(&mut output, "uptime", option_duration(diagnostics.uptime));
        line(
            &mut output,
            "logical_cpu_count",
            option_display(diagnostics.logical_cpu_count),
        );
        line(
            &mut output,
            "cpu_usage_percent",
            option_f32(diagnostics.cpu_usage_percent),
        );

        output.push_str("\n[health]\n");
        line(
            &mut output,
            "status",
            self.health.map_or("unavailable", health_name),
        );

        output.push_str("\n[measurement]\n");
        if let Some(measurement) = self.measurement.filter(valid_measurement) {
            line(
                &mut output,
                "offset_seconds",
                format_float(measurement.offset),
            );
            line(
                &mut output,
                "round_trip_delay_seconds",
                format_float(measurement.round_trip_delay),
            );
            line(
                &mut output,
                "root_distance_seconds",
                format_float(measurement.root_distance),
            );
        } else {
            line(&mut output, "offset_seconds", "unavailable");
            line(&mut output, "round_trip_delay_seconds", "unavailable");
            line(&mut output, "root_distance_seconds", "unavailable");
        }

        output.push_str("\n[server]\n");
        match &self.server {
            Some(server) => {
                line(&mut output, "name", &server.name);
                line(&mut output, "address", &server.address);
            }
            None => {
                line(&mut output, "name", "unavailable");
                line(&mut output, "address", "unavailable");
            }
        }

        output.push_str("\n[network]\n");
        if let Some(network) = &self.network {
            network_text(&mut output, "offset", network.offset_statistics());
            network_text(
                &mut output,
                "round_trip_time",
                network.round_trip_time_statistics(),
            );
            network_text(&mut output, "jitter", network.jitter_statistics());
            network_text(
                &mut output,
                "frequency_error",
                network.frequency_error_statistics(),
            );
            line(
                &mut output,
                "packet_loss_percent",
                network
                    .packet_loss_percent()
                    .map_or_else(|| "unavailable".to_owned(), format_float),
            );
        } else {
            for name in [
                "offset",
                "round_trip_time",
                "jitter",
                "frequency_error",
                "packet_loss_percent",
            ] {
                line(&mut output, name, "unavailable");
            }
        }
        output
    }

    /// Renders deterministic JSON-like output using valid JSON syntax.
    pub fn to_json(&self) -> String {
        let mut output = String::from("{\"version\":1,\"diagnostics\":{");
        field_string(
            &mut output,
            "uptime",
            option_duration(self.diagnostics.uptime),
            true,
        );
        field_string(
            &mut output,
            "logical_cpu_count",
            option_display(self.diagnostics.logical_cpu_count),
            false,
        );
        field_string(
            &mut output,
            "cpu_usage_percent",
            option_f32(self.diagnostics.cpu_usage_percent),
            false,
        );
        output.push_str("},\"health\":{");
        field_string(
            &mut output,
            "status",
            self.health.map_or("unavailable", health_name),
            true,
        );
        output.push_str("},\"measurement\":{");
        if let Some(measurement) = self.measurement.filter(valid_measurement) {
            field_number(&mut output, "offset_seconds", measurement.offset, true);
            field_number(
                &mut output,
                "round_trip_delay_seconds",
                measurement.round_trip_delay,
                false,
            );
            field_number(
                &mut output,
                "root_distance_seconds",
                measurement.root_distance,
                false,
            );
        } else {
            field_null(&mut output, "offset_seconds", true);
            field_null(&mut output, "round_trip_delay_seconds", false);
            field_null(&mut output, "root_distance_seconds", false);
        }
        output.push_str("},\"server\":");
        match &self.server {
            Some(server) => output.push_str(&format!(
                "{{\"name\":{},\"address\":{}}}",
                json_string(&server.name),
                json_string(&server.address)
            )),
            None => output.push_str("null"),
        }
        output.push_str(",\"network\":{");
        let stats = self.network.as_ref();
        json_stats(
            &mut output,
            "offset",
            stats.and_then(NetworkStatistics::offset_statistics),
            true,
        );
        json_stats(
            &mut output,
            "round_trip_time",
            stats.and_then(NetworkStatistics::round_trip_time_statistics),
            false,
        );
        json_stats(
            &mut output,
            "jitter",
            stats.and_then(NetworkStatistics::jitter_statistics),
            false,
        );
        json_stats(
            &mut output,
            "frequency_error",
            stats.and_then(NetworkStatistics::frequency_error_statistics),
            false,
        );
        match stats.and_then(NetworkStatistics::packet_loss_percent) {
            Some(value) => field_number(&mut output, "packet_loss_percent", value, false),
            None => field_null(&mut output, "packet_loss_percent", false),
        }
        output.push_str("}}");
        output
    }

    pub fn write_plain_text<P: AsRef<Path>>(&self, path: P) -> Result<(), ExportError> {
        atomic_write(path.as_ref(), self.to_plain_text().as_bytes())
    }

    pub fn write_json<P: AsRef<Path>>(&self, path: P) -> Result<(), ExportError> {
        atomic_write(path.as_ref(), self.to_json().as_bytes())
    }
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), ExportError> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("export");
    let mut temporary = None;
    for _ in 0..16 {
        let candidate = parent.join(format!(
            ".{name}.{}.tmp",
            TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                temporary = Some((candidate, file));
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(source) => return Err(io_error("create temporary export", candidate, source)),
        }
    }
    let (temporary, mut file) = temporary.ok_or_else(|| {
        io_error(
            "reserve temporary export",
            path.to_owned(),
            io::Error::new(io::ErrorKind::AlreadyExists, "too many temporary files"),
        )
    })?;
    let result = (|| {
        file.write_all(contents)
            .map_err(|source| io_error("write temporary export", temporary.clone(), source))?;
        file.sync_all()
            .map_err(|source| io_error("sync temporary export", temporary.clone(), source))?;
        drop(file);
        replace_file(&temporary, path)
            .map_err(|source| io_error("replace export", path.to_owned(), source))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(not(windows))]
fn replace_file(temporary: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(temporary, destination)
}

#[cfg(windows)]
fn replace_file(temporary: &Path, destination: &Path) -> io::Result<()> {
    let temporary: Vec<u16> = temporary
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let replaced = unsafe {
        MoveFileExW(
            temporary.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if replaced == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
const MOVEFILE_REPLACE_EXISTING: u32 = 0x0000_0001;
#[cfg(windows)]
const MOVEFILE_WRITE_THROUGH: u32 = 0x0000_0008;

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn MoveFileExW(existing_file_name: *const u16, new_file_name: *const u16, flags: u32) -> i32;
}

fn io_error(operation: &'static str, path: PathBuf, source: io::Error) -> ExportError {
    ExportError::Io {
        operation,
        path,
        source,
    }
}
fn valid_measurement(value: &Measurement) -> bool {
    value.offset.is_finite()
        && value.round_trip_delay.is_finite()
        && value.root_distance.is_finite()
}
fn health_name(value: HealthStatus) -> &'static str {
    match value {
        HealthStatus::Synchronized => "synchronized",
        HealthStatus::Uncertain => "uncertain",
        HealthStatus::Unavailable => "unavailable",
    }
}
fn format_float(value: f64) -> String {
    format!("{value:.6}")
}
fn option_duration(value: Option<Duration>) -> String {
    value.map_or_else(|| "unavailable".to_owned(), |v| v.as_secs().to_string())
}
fn option_display<T: ToString>(value: Option<T>) -> String {
    value.map_or_else(|| "unavailable".to_owned(), |v| v.to_string())
}
fn option_f32(value: Option<f32>) -> String {
    value
        .filter(|v| v.is_finite())
        .map_or_else(|| "unavailable".to_owned(), |v| format_float(f64::from(v)))
}
fn line(output: &mut String, key: &str, value: impl AsRef<str>) {
    output.push_str(key);
    output.push('=');
    output.push_str(value.as_ref());
    output.push('\n');
}
fn network_text(output: &mut String, name: &str, stats: Option<Statistics>) {
    if let Some(stats) = stats {
        line(output, &format!("{name}_min"), format_float(stats.min));
        line(output, &format!("{name}_max"), format_float(stats.max));
        line(output, &format!("{name}_mean"), format_float(stats.mean));
        line(
            output,
            &format!("{name}_standard_deviation"),
            format_float(stats.standard_deviation),
        );
    } else {
        line(output, &format!("{name}_min"), "unavailable");
        line(output, &format!("{name}_max"), "unavailable");
        line(output, &format!("{name}_mean"), "unavailable");
        line(output, &format!("{name}_standard_deviation"), "unavailable");
    }
}
fn json_string(value: &str) -> String {
    let mut result = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c.is_control() => result.push_str(&format!("\\u{:04x}", c as u32)),
            c => result.push(c),
        }
    }
    result.push('"');
    result
}
fn field_string(output: &mut String, key: &str, value: impl AsRef<str>, first: bool) {
    if !first {
        output.push(',');
    }
    output.push_str(&json_string(key));
    output.push(':');
    output.push_str(&json_string(value.as_ref()));
}
fn field_number(output: &mut String, key: &str, value: f64, first: bool) {
    if !first {
        output.push(',');
    }
    output.push_str(&json_string(key));
    output.push(':');
    output.push_str(&format_float(value));
}
fn field_null(output: &mut String, key: &str, first: bool) {
    if !first {
        output.push(',');
    }
    output.push_str(&json_string(key));
    output.push_str(":null");
}
fn json_stats(output: &mut String, name: &str, stats: Option<Statistics>, first: bool) {
    if !first {
        output.push(',');
    }
    output.push_str(&json_string(name));
    output.push(':');
    match stats {
        Some(stats) => {
            output.push('{');
            field_number(output, "min", stats.min, true);
            field_number(output, "max", stats.max, false);
            field_number(output, "mean", stats.mean, false);
            field_number(
                output,
                "standard_deviation",
                stats.standard_deviation,
                false,
            );
            output.push('}');
        }
        None => output.push_str("null"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn path(suffix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "master-time-export-{}-{suffix}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn formats_all_sections_stably() {
        let export = DiagnosticSnapshot {
            diagnostics: DiagnosticsSnapshot {
                uptime: Some(Duration::from_secs(12)),
                logical_cpu_count: Some(4),
                cpu_usage_percent: Some(25.0),
            },
            health: Some(HealthStatus::Synchronized),
            measurement: Some(Measurement {
                offset: 0.001,
                round_trip_delay: 0.002,
                root_distance: 0.003,
            }),
            server: Some(ServerInfo::new("Primary", "127.0.0.1:123")),
            network: None,
        };
        let text = export.to_plain_text();
        assert!(text.contains("[diagnostics]\nuptime=12\n"));
        assert!(text.contains("[health]\nstatus=synchronized\n"));
        assert!(text.contains("name=Primary\naddress=127.0.0.1:123\n"));
        assert!(export.to_json().starts_with("{\"version\":1,"));
    }

    #[test]
    fn writes_plain_text_atomically_to_a_temporary_file() {
        let target = path("plain.txt");
        let export = DiagnosticSnapshot::default();
        export.write_plain_text(&target).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), export.to_plain_text());
        let _ = fs::remove_file(target);
    }

    #[test]
    fn overwrites_existing_plain_text_and_json_exports() {
        let text_target = path("overwrite.txt");
        let json_target = path("overwrite.json");
        let first = DiagnosticSnapshot::default();
        let second = DiagnosticSnapshot {
            diagnostics: DiagnosticsSnapshot {
                uptime: Some(Duration::from_secs(42)),
                ..DiagnosticsSnapshot::default()
            },
            ..DiagnosticSnapshot::default()
        };

        first.write_plain_text(&text_target).unwrap();
        second.write_plain_text(&text_target).unwrap();
        assert_eq!(
            fs::read_to_string(&text_target).unwrap(),
            second.to_plain_text()
        );

        first.write_json(&json_target).unwrap();
        second.write_json(&json_target).unwrap();
        assert_eq!(fs::read_to_string(&json_target).unwrap(), second.to_json());

        let _ = fs::remove_file(text_target);
        let _ = fs::remove_file(json_target);
    }

    #[test]
    fn reports_clear_errors_for_missing_parent() {
        let target = path("missing-parent/export.json");
        let error = DiagnosticSnapshot::default()
            .write_json(&target)
            .unwrap_err();
        assert!(error.to_string().contains("create temporary export"));
    }
}
