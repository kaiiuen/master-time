//! Small, framework-neutral localization primitives for the desktop UI.
//!
//! English is intentionally represented as a match over the typed key set. This
//! keeps the default language complete without maintaining a large string table.

/// Stable identifiers for text shown by the desktop application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Server,
    ServerName,
    ServerAddress,
    SyncStatus,
    Synchronized,
    Uncertain,
    Unavailable,
    Metrics,
    Stratum,
    Offset,
    RoundTripDelay,
    RootDistance,
    Polling,
    PollingInterval,
    PollNow,
    PollingEnabled,
    Settings,
    SaveSettings,
    ResetSettings,
    Errors,
    Error,
    ConnectionError,
    InvalidPollingInterval,
    NoMeasurement,
}

impl Key {
    /// Returns the stable identifier used when a key comes from UI metadata.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::ServerName => "server_name",
            Self::ServerAddress => "server_address",
            Self::SyncStatus => "sync_status",
            Self::Synchronized => "synchronized",
            Self::Uncertain => "uncertain",
            Self::Unavailable => "unavailable",
            Self::Metrics => "metrics",
            Self::Stratum => "stratum",
            Self::Offset => "offset",
            Self::RoundTripDelay => "round_trip_delay",
            Self::RootDistance => "root_distance",
            Self::Polling => "polling",
            Self::PollingInterval => "polling_interval",
            Self::PollNow => "poll_now",
            Self::PollingEnabled => "polling_enabled",
            Self::Settings => "settings",
            Self::SaveSettings => "save_settings",
            Self::ResetSettings => "reset_settings",
            Self::Errors => "errors",
            Self::Error => "error",
            Self::ConnectionError => "connection_error",
            Self::InvalidPollingInterval => "invalid_polling_interval",
            Self::NoMeasurement => "no_measurement",
        }
    }
}

/// The complete built-in English language.
#[derive(Debug, Clone, Copy, Default)]
pub struct English;

impl English {
    /// Looks up a known, typed UI key.
    pub const fn text(key: Key) -> &'static str {
        match key {
            Key::Server => "Server",
            Key::ServerName => "Server name",
            Key::ServerAddress => "Server address",
            Key::SyncStatus => "Sync status",
            Key::Synchronized => "Synchronized",
            Key::Uncertain => "Uncertain",
            Key::Unavailable => "Unavailable",
            Key::Metrics => "Metrics",
            Key::Stratum => "Stratum",
            Key::Offset => "Offset",
            Key::RoundTripDelay => "Round-trip delay",
            Key::RootDistance => "Root distance",
            Key::Polling => "Polling",
            Key::PollingInterval => "Polling interval",
            Key::PollNow => "Poll now",
            Key::PollingEnabled => "Polling enabled",
            Key::Settings => "Settings",
            Key::SaveSettings => "Save settings",
            Key::ResetSettings => "Reset settings",
            Key::Errors => "Errors",
            Key::Error => "Error",
            Key::ConnectionError => "Connection error",
            Key::InvalidPollingInterval => "Invalid polling interval",
            Key::NoMeasurement => "No measurement available",
        }
    }

    /// Looks up a UI key supplied as metadata, using a safe fixed fallback.
    ///
    /// Unknown input is never rendered as UI text. This makes missing keys
    /// visible without allowing arbitrary metadata to become a label.
    pub fn text_for(id: &str) -> &'static str {
        match id {
            "server" => Self::text(Key::Server),
            "server_name" => Self::text(Key::ServerName),
            "server_address" => Self::text(Key::ServerAddress),
            "sync_status" => Self::text(Key::SyncStatus),
            "synchronized" => Self::text(Key::Synchronized),
            "uncertain" => Self::text(Key::Uncertain),
            "unavailable" => Self::text(Key::Unavailable),
            "metrics" => Self::text(Key::Metrics),
            "stratum" => Self::text(Key::Stratum),
            "offset" => Self::text(Key::Offset),
            "round_trip_delay" => Self::text(Key::RoundTripDelay),
            "root_distance" => Self::text(Key::RootDistance),
            "polling" => Self::text(Key::Polling),
            "polling_interval" => Self::text(Key::PollingInterval),
            "poll_now" => Self::text(Key::PollNow),
            "polling_enabled" => Self::text(Key::PollingEnabled),
            "settings" => Self::text(Key::Settings),
            "save_settings" => Self::text(Key::SaveSettings),
            "reset_settings" => Self::text(Key::ResetSettings),
            "errors" => Self::text(Key::Errors),
            "error" => Self::text(Key::Error),
            "connection_error" => Self::text(Key::ConnectionError),
            "invalid_polling_interval" => Self::text(Key::InvalidPollingInterval),
            "no_measurement" => Self::text(Key::NoMeasurement),
            _ => MISSING_TRANSLATION,
        }
    }
}

/// Fixed text used when a dynamic or stale key has no translation.
pub const MISSING_TRANSLATION: &str = "Missing translation";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_known_keys() {
        assert_eq!(English::text(Key::Server), "Server");
        assert_eq!(English::text(Key::SyncStatus), "Sync status");
        assert_eq!(English::text(Key::RoundTripDelay), "Round-trip delay");
        assert_eq!(English::text(Key::PollingInterval), "Polling interval");
        assert_eq!(English::text(Key::Settings), "Settings");
        assert_eq!(English::text(Key::ConnectionError), "Connection error");
    }

    #[test]
    fn resolves_ids_and_falls_back_safely() {
        assert_eq!(English::text_for(Key::Metrics.id()), "Metrics");
        assert_eq!(English::text_for("not_a_real_key"), MISSING_TRANSLATION);
        assert_eq!(
            English::text_for("<script>alert(1)</script>"),
            MISSING_TRANSLATION
        );
    }
}
