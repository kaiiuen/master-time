//! In-memory application configuration.
//!
//! This module deliberately contains no persistence or serialization code. It
//! provides validated values that a user interface or persistence layer can
//! build on later.

use std::{fmt, time::Duration};

/// The default interval between polling attempts.
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(60);
/// The shortest interval accepted for polling.
pub const MIN_POLL_INTERVAL: Duration = Duration::from_secs(5);
/// The longest interval accepted for polling.
pub const MAX_POLL_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// A named NTP server that can be selected by the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerProfile {
    name: String,
    hostname: String,
}

impl ServerProfile {
    /// Creates a server profile after validating its name and hostname.
    pub fn new(name: impl Into<String>, hostname: impl Into<String>) -> Result<Self, ConfigError> {
        let name = name.into();
        let hostname = hostname.into();

        if name.trim().is_empty() {
            return Err(ConfigError::EmptyServerName);
        }
        if name.chars().any(char::is_control) {
            return Err(ConfigError::InvalidServerName);
        }
        validate_hostname(&hostname)?;

        Ok(Self { name, hostname })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn hostname(&self) -> &str {
        &self.hostname
    }
}

/// User preferences controlling how often the active server is polled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PollingPreferences {
    interval: Duration,
}

impl Default for PollingPreferences {
    fn default() -> Self {
        Self {
            interval: DEFAULT_POLL_INTERVAL,
        }
    }
}

impl PollingPreferences {
    /// Creates preferences with an interval inside the supported bounds.
    pub fn new(interval: Duration) -> Result<Self, ConfigError> {
        validate_interval(interval)?;
        Ok(Self { interval })
    }

    pub const fn interval(&self) -> Duration {
        self.interval
    }

    /// Updates the interval, leaving the existing value unchanged on failure.
    pub fn set_interval(&mut self, interval: Duration) -> Result<(), ConfigError> {
        validate_interval(interval)?;
        self.interval = interval;
        Ok(())
    }
}

/// A validated, in-memory application configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    servers: Vec<ServerProfile>,
    active_server: Option<usize>,
    polling: PollingPreferences,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            servers: Vec::new(),
            active_server: None,
            polling: PollingPreferences::default(),
        }
    }
}

impl AppConfig {
    /// Creates a configuration and validates the active server selection.
    pub fn new(
        servers: Vec<ServerProfile>,
        active_server: Option<usize>,
        polling: PollingPreferences,
    ) -> Result<Self, ConfigError> {
        if let Some(index) = active_server {
            if index >= servers.len() {
                return Err(ConfigError::ActiveServerOutOfBounds {
                    index,
                    server_count: servers.len(),
                });
            }
        }

        Ok(Self {
            servers,
            active_server,
            polling,
        })
    }

    pub fn servers(&self) -> &[ServerProfile] {
        &self.servers
    }

    pub const fn active_server_index(&self) -> Option<usize> {
        self.active_server
    }

    pub fn active_server(&self) -> Option<&ServerProfile> {
        self.active_server.and_then(|index| self.servers.get(index))
    }

    pub const fn polling(&self) -> PollingPreferences {
        self.polling
    }

    /// Adds a server and selects it when no server is currently active.
    pub fn add_server(&mut self, server: ServerProfile) {
        self.servers.push(server);
        if self.active_server.is_none() {
            self.active_server = Some(self.servers.len() - 1);
        }
    }

    /// Selects a server by index, or clears the selection with `None`.
    pub fn set_active_server(&mut self, index: Option<usize>) -> Result<(), ConfigError> {
        if let Some(index) = index {
            if index >= self.servers.len() {
                return Err(ConfigError::ActiveServerOutOfBounds {
                    index,
                    server_count: self.servers.len(),
                });
            }
        }
        self.active_server = index;
        Ok(())
    }

    pub fn set_polling(&mut self, polling: PollingPreferences) {
        self.polling = polling;
    }
}

/// Errors returned when configuration values are invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    EmptyServerName,
    InvalidServerName,
    EmptyHostname,
    InvalidHostname,
    InvalidHostnameLabel,
    PollIntervalTooShort { minimum: Duration, actual: Duration },
    PollIntervalTooLong { maximum: Duration, actual: Duration },
    ActiveServerOutOfBounds { index: usize, server_count: usize },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyServerName => formatter.write_str("server name cannot be empty"),
            Self::InvalidServerName => {
                formatter.write_str("server name contains a control character")
            }
            Self::EmptyHostname => formatter.write_str("hostname cannot be empty"),
            Self::InvalidHostname => formatter.write_str("hostname contains invalid characters"),
            Self::InvalidHostnameLabel => formatter.write_str("hostname label is invalid"),
            Self::PollIntervalTooShort { minimum, actual } => {
                write!(
                    formatter,
                    "poll interval {actual:?} is shorter than minimum {minimum:?}"
                )
            }
            Self::PollIntervalTooLong { maximum, actual } => {
                write!(
                    formatter,
                    "poll interval {actual:?} is longer than maximum {maximum:?}"
                )
            }
            Self::ActiveServerOutOfBounds {
                index,
                server_count,
            } => write!(
                formatter,
                "active server index {index} is invalid for {server_count} servers"
            ),
        }
    }
}

impl std::error::Error for ConfigError {}

fn validate_interval(interval: Duration) -> Result<(), ConfigError> {
    if interval < MIN_POLL_INTERVAL {
        return Err(ConfigError::PollIntervalTooShort {
            minimum: MIN_POLL_INTERVAL,
            actual: interval,
        });
    }
    if interval > MAX_POLL_INTERVAL {
        return Err(ConfigError::PollIntervalTooLong {
            maximum: MAX_POLL_INTERVAL,
            actual: interval,
        });
    }
    Ok(())
}

fn validate_hostname(hostname: &str) -> Result<(), ConfigError> {
    if hostname.is_empty() {
        return Err(ConfigError::EmptyHostname);
    }
    if !hostname.is_ascii() {
        return Err(ConfigError::InvalidHostname);
    }
    if hostname.starts_with('.') || hostname.ends_with('.') {
        return Err(ConfigError::InvalidHostname);
    }

    for label in hostname.split('.') {
        if label.is_empty() || label.len() > 63 || label.starts_with('-') || label.ends_with('-') {
            return Err(ConfigError::InvalidHostnameLabel);
        }
        if !label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(ConfigError::InvalidHostname);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_empty_and_poll_once_per_minute() {
        let config = AppConfig::default();

        assert!(config.servers().is_empty());
        assert_eq!(config.active_server_index(), None);
        assert_eq!(config.polling().interval(), DEFAULT_POLL_INTERVAL);
        assert_eq!(
            PollingPreferences::default().interval(),
            Duration::from_secs(60)
        );
    }

    #[test]
    fn rejects_polling_intervals_outside_bounds() {
        assert_eq!(
            PollingPreferences::new(Duration::ZERO).unwrap_err(),
            ConfigError::PollIntervalTooShort {
                minimum: MIN_POLL_INTERVAL,
                actual: Duration::ZERO,
            }
        );
        assert!(matches!(
            PollingPreferences::new(MAX_POLL_INTERVAL + Duration::from_secs(1)),
            Err(ConfigError::PollIntervalTooLong { .. })
        ));
    }

    #[test]
    fn rejects_invalid_profiles_and_active_selection() {
        assert!(matches!(
            ServerProfile::new("", "time.example.test"),
            Err(ConfigError::EmptyServerName)
        ));
        assert!(matches!(
            ServerProfile::new("Example", "-invalid.example"),
            Err(ConfigError::InvalidHostnameLabel)
        ));

        let server = ServerProfile::new("Example", "time.example.test").unwrap();
        assert!(matches!(
            AppConfig::new(vec![server], Some(1), PollingPreferences::default()),
            Err(ConfigError::ActiveServerOutOfBounds { .. })
        ));
    }
}
