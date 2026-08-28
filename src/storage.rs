//! Dependency-free persistence for [`AppConfig`] using a small text format.
//!
//! The file format is UTF-8 and consists of one `key=value` record per line:
//!
//! ```text
//! # Master Time configuration
//! version=1
//! poll_interval_secs=60
//! active_server=none
//! server_count=0
//! ```
//!
//! A server record is `server=<percent-encoded name>\t<hostname>`. Percent
//! encoding uses two uppercase hexadecimal digits for every byte outside the
//! ASCII unreserved set (`A-Z`, `a-z`, `0-9`, `-._~`). This keeps names with
//! spaces, `=`, or non-ASCII characters unambiguous while leaving the format
//! easy to inspect and edit. Unknown keys, duplicate keys, missing fields, and
//! inconsistent server counts are rejected.

use crate::config::{AppConfig, ConfigError, PollingPreferences, ServerProfile};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const FORMAT_VERSION: &str = "1";
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Errors produced while saving or loading an application configuration.
#[derive(Debug)]
pub enum StorageError {
    Io {
        operation: &'static str,
        source: io::Error,
    },
    InvalidFormat {
        line: Option<usize>,
        message: String,
    },
    InvalidConfig {
        source: ConfigError,
    },
}

impl fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { operation, source } => {
                write!(formatter, "could not {operation} configuration: {source}")
            }
            Self::InvalidFormat {
                line: Some(line),
                message,
            } => {
                write!(formatter, "invalid configuration on line {line}: {message}")
            }
            Self::InvalidFormat {
                line: None,
                message,
            } => write!(formatter, "invalid configuration: {message}"),
            Self::InvalidConfig { source } => {
                write!(formatter, "configuration validation failed: {source}")
            }
        }
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::InvalidConfig { source } => Some(source),
            Self::InvalidFormat { .. } => None,
        }
    }
}

/// Saves `config` atomically to `path`.
///
/// The temporary file is created in the destination directory, flushed to
/// disk, and renamed into place. A failed save leaves the existing file alone.
pub fn save_config(path: impl AsRef<Path>, config: &AppConfig) -> Result<(), StorageError> {
    let path = path.as_ref();
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let contents = serialize(config);
    let mut temporary = None;
    let mut file = None;

    for _ in 0..100 {
        let candidate = temporary_path(parent, path);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(created) => {
                temporary = Some(candidate);
                file = Some(created);
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(source) => return Err(io_error("create temporary", source)),
        }
    }

    let temporary = temporary.ok_or_else(|| StorageError::InvalidFormat {
        line: None,
        message: "unable to create a unique temporary file".to_owned(),
    })?;
    let result = write_temporary(file.expect("temporary file exists"), &contents)
        .and_then(|()| atomic_replace(&temporary, path));

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Loads and validates a configuration from `path`.
pub fn load_config(path: impl AsRef<Path>) -> Result<AppConfig, StorageError> {
    let path = path.as_ref();
    let mut contents = String::new();
    File::open(path)
        .map_err(|source| io_error("open", source))?
        .read_to_string(&mut contents)
        .map_err(|source| io_error("read", source))?;
    parse(&contents)
}

/// Short alias for [`save_config`].
pub fn save(path: impl AsRef<Path>, config: &AppConfig) -> Result<(), StorageError> {
    save_config(path, config)
}

/// Short alias for [`load_config`].
pub fn load(path: impl AsRef<Path>) -> Result<AppConfig, StorageError> {
    load_config(path)
}

fn serialize(config: &AppConfig) -> String {
    let mut output = String::from("# Master Time configuration\nversion=1\n");
    output.push_str(&format!(
        "poll_interval_secs={}\n",
        config.polling().interval().as_secs()
    ));
    match config.active_server_index() {
        Some(index) => output.push_str(&format!("active_server={index}\n")),
        None => output.push_str("active_server=none\n"),
    }
    output.push_str(&format!("server_count={}\n", config.servers().len()));
    for server in config.servers() {
        output.push_str("server=");
        output.push_str(&encode(server.name()));
        output.push('\t');
        output.push_str(server.hostname());
        output.push('\n');
    }
    output
}

fn parse(contents: &str) -> Result<AppConfig, StorageError> {
    let mut version = None;
    let mut interval = None;
    let mut active = None;
    let mut count = None;
    let mut servers = Vec::new();

    for (line_number, raw_line) in contents.lines().enumerate() {
        let line_number = line_number + 1;
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format_error(line_number, "expected key=value"))?;
        match key {
            "version" => set_once(&mut version, value, line_number, "version")?,
            "poll_interval_secs" => {
                set_once(&mut interval, value, line_number, "poll_interval_secs")?
            }
            "active_server" => set_once(&mut active, value, line_number, "active_server")?,
            "server_count" => set_once(&mut count, value, line_number, "server_count")?,
            "server" => {
                let (name, hostname) = value.split_once('\t').ok_or_else(|| {
                    format_error(line_number, "server must contain encoded name and hostname")
                })?;
                if hostname.contains('\t') || hostname.is_empty() {
                    return Err(format_error(line_number, "server hostname is malformed"));
                }
                let name = decode(name).map_err(|message| format_error(line_number, &message))?;
                let server = ServerProfile::new(name, hostname)
                    .map_err(|source| StorageError::InvalidConfig { source })?;
                servers.push(server);
            }
            _ => return Err(format_error(line_number, "unknown key")),
        }
    }

    let version = version.ok_or_else(|| missing("version"))?;
    if version != FORMAT_VERSION {
        return Err(format_error(0, "unsupported format version"));
    }
    let interval = parse_u64(
        interval.ok_or_else(|| missing("poll_interval_secs"))?,
        "poll interval",
    )?;
    let active_value = active.ok_or_else(|| missing("active_server"))?;
    let active = if active_value == "none" {
        None
    } else {
        Some(parse_usize(active_value, "active server index")?)
    };
    let expected = parse_usize(
        count.ok_or_else(|| missing("server_count"))?,
        "server count",
    )?;
    if expected != servers.len() {
        return Err(format_error(
            0,
            "server_count does not match server records",
        ));
    }
    let polling = PollingPreferences::new(Duration::from_secs(interval))
        .map_err(|source| StorageError::InvalidConfig { source })?;
    AppConfig::new(servers, active, polling)
        .map_err(|source| StorageError::InvalidConfig { source })
}

fn set_once<'a>(
    slot: &mut Option<&'a str>,
    value: &'a str,
    line: usize,
    name: &str,
) -> Result<(), StorageError> {
    if slot.replace(value).is_some() {
        Err(format_error(line, &format!("duplicate {name}")))
    } else {
        Ok(())
    }
}

fn parse_u64(value: &str, what: &str) -> Result<u64, StorageError> {
    value
        .parse()
        .map_err(|_| format_error(0, &format!("{what} is not an unsigned integer")))
}

fn parse_usize(value: &str, what: &str) -> Result<usize, StorageError> {
    value
        .parse()
        .map_err(|_| format_error(0, &format!("{what} is not a valid integer")))
}

fn format_error(line: usize, message: &str) -> StorageError {
    StorageError::InvalidFormat {
        line: (line != 0).then_some(line),
        message: message.to_owned(),
    }
}

fn missing(field: &str) -> StorageError {
    format_error(0, &format!("missing {field}"))
}

fn io_error(operation: &'static str, source: io::Error) -> StorageError {
    StorageError::Io { operation, source }
}

fn temporary_path(parent: &Path, destination: &Path) -> PathBuf {
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config");
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let number = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(".{name}.tmp-{stamp}-{number}"))
}

fn write_temporary(mut file: File, contents: &str) -> Result<(), StorageError> {
    file.write_all(contents.as_bytes())
        .map_err(|source| io_error("write", source))?;
    file.sync_all()
        .map_err(|source| io_error("flush", source))?;
    drop(file);
    Ok(())
}

#[cfg(not(windows))]
fn atomic_replace(temporary: &Path, destination: &Path) -> Result<(), StorageError> {
    fs::rename(temporary, destination).map_err(|source| io_error("rename", source))
}

#[cfg(windows)]
fn atomic_replace(temporary: &Path, destination: &Path) -> Result<(), StorageError> {
    use std::os::windows::ffi::OsStrExt;
    let temporary: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    const MOVEFILE_REPLACE_EXISTING: u32 = 2;
    const MOVEFILE_WRITE_THROUGH: u32 = 8;
    const REPLACEFILE_WRITE_THROUGH: u32 = 1;
    unsafe extern "system" {
        fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
        fn ReplaceFileW(
            replaced: *const u16,
            replacement: *const u16,
            backup: *const u16,
            flags: u32,
            exclude: *const std::ffi::c_void,
            reserved: *const std::ffi::c_void,
        ) -> i32;
    }
    // ReplaceFileW is the Windows atomic replacement primitive when the target exists.
    let moved = unsafe {
        MoveFileExW(
            temporary.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved != 0 {
        return Ok(());
    }
    let move_error = io::Error::last_os_error();
    let replaced = unsafe {
        ReplaceFileW(
            destination.as_ptr(),
            temporary.as_ptr(),
            std::ptr::null(),
            REPLACEFILE_WRITE_THROUGH,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if replaced != 0 {
        Ok(())
    } else {
        let replace_error = io::Error::last_os_error();
        Err(io_error(
            "rename",
            io::Error::new(
                replace_error.kind(),
                format!("{replace_error} (initial rename: {move_error})"),
            ),
        ))
    }
}

fn encode(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            output.push(byte as char);
        } else {
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    output
}

fn decode(value: &str) -> Result<String, String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err("incomplete percent escape".to_owned());
            }
            let high = hex(bytes[index + 1]).ok_or_else(|| "invalid percent escape".to_owned())?;
            let low = hex(bytes[index + 2]).ok_or_else(|| "invalid percent escape".to_owned())?;
            decoded.push(high << 4 | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| "server name is not valid UTF-8".to_owned())
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn path(suffix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "master-time-storage-test-{}-{suffix}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn sample() -> AppConfig {
        let mut config = AppConfig::default();
        config.add_server(ServerProfile::new("A name = 日本", "time.example.test").unwrap());
        config.add_server(ServerProfile::new("Backup", "backup.example.test").unwrap());
        config.set_polling(PollingPreferences::new(Duration::from_secs(30)).unwrap());
        config
    }

    #[test]
    fn round_trips_and_replaces_existing_file() {
        let file = path("config");
        save_config(&file, &sample()).unwrap();
        save_config(&file, &AppConfig::default()).unwrap();
        assert_eq!(load_config(&file).unwrap(), AppConfig::default());
        let _ = fs::remove_file(file);
    }

    #[test]
    fn rejects_invalid_and_incomplete_input() {
        let error = parse("version=1\npoll_interval_secs=1\nactive_server=none\nserver_count=0\n")
            .unwrap_err();
        assert!(matches!(error, StorageError::InvalidConfig { .. }));
        let error = parse("version=1\npoll_interval_secs=60\nactive_server=0\nserver_count=0\n")
            .unwrap_err();
        assert!(matches!(
            error,
            StorageError::InvalidConfig {
                source: ConfigError::ActiveServerOutOfBounds { .. }
            }
        ));
        assert!(matches!(
            parse("version=2\npoll_interval_secs=60\nactive_server=none\nserver_count=0"),
            Err(StorageError::InvalidFormat { .. })
        ));
    }

    #[test]
    fn leaves_destination_unchanged_when_save_fails_before_rename() {
        let directory = path("directory");
        fs::create_dir_all(&directory).unwrap();
        let destination = directory.join("config");
        fs::write(&destination, b"old").unwrap();
        let error = save_config(&directory, &sample()).unwrap_err();
        assert!(matches!(error, StorageError::Io { .. }));
        assert_eq!(fs::read(&destination).unwrap(), b"old");
        let _ = fs::remove_dir_all(directory);
    }
}
