//! Application-owned state for the Master Time utility.
//!
//! This module contains no UI or scheduling dependencies. It coordinates the
//! configuration and the results produced by the measurement service so a UI
//! can read a consistent snapshot of the application state.

use crate::config::{AppConfig, ConfigError};
use crate::health::{self, HealthInput, HealthStatus, LeapIndicator};
use crate::measurement::MeasurementHistory;
use crate::service::{MeasurementResult, ServiceError};

/// The default number of offset samples retained by [`ApplicationState`].
pub const DEFAULT_HISTORY_CAPACITY: usize = 120;

/// The lifecycle state of a polling attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollingState {
    /// No poll is currently in progress.
    Idle,
    /// A poll has been started and is awaiting a service result.
    Polling,
}

/// In-memory state shared by the application and its UI.
#[derive(Debug)]
pub struct ApplicationState {
    config: AppConfig,
    latest_measurement: Option<MeasurementResult>,
    health_status: HealthStatus,
    history: MeasurementHistory,
    connection_error: Option<ServiceError>,
    polling_state: PollingState,
}

/// Short alias for callers that prefer the type name `AppState`.
pub type AppState = ApplicationState;

impl ApplicationState {
    /// Creates state with an empty result set and a bounded offset history.
    pub fn new(config: AppConfig, history_capacity: usize) -> Self {
        Self {
            config,
            latest_measurement: None,
            health_status: HealthStatus::Unavailable,
            history: MeasurementHistory::new(history_capacity),
            connection_error: None,
            polling_state: PollingState::Idle,
        }
    }

    /// Returns the application configuration as a read-only view.
    pub const fn config(&self) -> &AppConfig {
        &self.config
    }

    /// Returns the latest successful measurement, if any.
    pub const fn latest_measurement(&self) -> Option<&MeasurementResult> {
        self.latest_measurement.as_ref()
    }

    /// Returns the health computed from the latest successful result.
    pub const fn health_status(&self) -> HealthStatus {
        self.health_status
    }

    /// Returns the rolling measurement history as a read-only view.
    pub const fn history(&self) -> &MeasurementHistory {
        &self.history
    }

    /// Returns the most recent service error, if the last attempt failed.
    pub const fn connection_error(&self) -> Option<&ServiceError> {
        self.connection_error.as_ref()
    }

    pub const fn polling_state(&self) -> PollingState {
        self.polling_state
    }

    /// Marks the beginning of a service request.
    pub fn begin_polling(&mut self) {
        self.polling_state = PollingState::Polling;
    }

    /// Applies a successful service result and updates health and history.
    pub fn apply_success(&mut self, result: MeasurementResult) {
        let leap_indicator =
            LeapIndicator::from_bits(result.header.leap_indicator).unwrap_or(LeapIndicator::Alarm);
        self.health_status = health::evaluate(HealthInput::new(
            true,
            result.header.stratum,
            leap_indicator,
            result.measurement.root_distance,
        ));
        self.history.push(result.measurement.offset);
        self.latest_measurement = Some(result);
        self.connection_error = None;
        self.polling_state = PollingState::Idle;
    }

    /// Records a failed service request without discarding the last success.
    pub fn record_error(&mut self, error: ServiceError) {
        self.connection_error = Some(error);
        self.health_status = HealthStatus::Unavailable;
        self.polling_state = PollingState::Idle;
    }

    /// Selects a server after validation, resetting result data tied to the old server.
    ///
    /// An invalid index returns an error and leaves every part of the state
    /// unchanged. Clearing the selection with `None` is supported.
    pub fn set_active_server(&mut self, index: Option<usize>) -> Result<(), ConfigError> {
        if index == self.config.active_server_index() {
            return Ok(());
        }
        self.config.set_active_server(index)?;
        self.latest_measurement = None;
        self.health_status = HealthStatus::Unavailable;
        self.history = MeasurementHistory::new(self.history.capacity());
        self.connection_error = None;
        self.polling_state = PollingState::Idle;
        Ok(())
    }

    /// Returns the configured active server without exposing mutable config.
    pub fn active_server(&self) -> Option<&crate::config::ServerProfile> {
        self.config.active_server()
    }
}

impl Default for ApplicationState {
    fn default() -> Self {
        Self::new(AppConfig::default(), DEFAULT_HISTORY_CAPACITY)
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use super::*;
    use crate::config::ServerProfile;
    use crate::measurement::{FourTimestamps, Measurement, NtpTimestamp};
    use crate::ntp::{NtpHeader, NtpTimestamp as PacketTimestamp};

    fn result(offset: f64, stratum: u8, root_distance: f64) -> MeasurementResult {
        MeasurementResult {
            server: "127.0.0.1:123".parse::<SocketAddr>().unwrap(),
            header: NtpHeader {
                leap_indicator: 0,
                version: 4,
                mode: 4,
                stratum,
                poll_exponent: 0,
                precision_exponent: 0,
                root_delay: 0,
                root_dispersion: 0,
                reference_id: [0; 4],
                reference_timestamp: PacketTimestamp::ZERO,
                originate_timestamp: PacketTimestamp::ZERO,
                receive_timestamp: PacketTimestamp::ZERO,
                transmit_timestamp: PacketTimestamp::ZERO,
            },
            timestamps: FourTimestamps::default(),
            measurement: Measurement {
                offset,
                round_trip_delay: 0.01,
                root_distance,
            },
        }
    }

    fn config_with_servers() -> AppConfig {
        let mut config = AppConfig::default();
        config.add_server(ServerProfile::new("one", "one.example").unwrap());
        config.add_server(ServerProfile::new("two", "two.example").unwrap());
        config
    }

    #[test]
    fn success_updates_measurement_health_and_clears_error() {
        let mut state = ApplicationState::new(AppConfig::default(), 4);
        state.record_error(ServiceError::UnsupportedMode(3));
        state.begin_polling();
        state.apply_success(result(0.25, 2, 0.5));

        assert_eq!(state.latest_measurement().unwrap().measurement.offset, 0.25);
        assert_eq!(state.health_status(), HealthStatus::Synchronized);
        assert!(state.connection_error().is_none());
        assert_eq!(state.polling_state(), PollingState::Idle);
    }

    #[test]
    fn failure_records_error_and_preserves_last_success() {
        let mut state = ApplicationState::new(AppConfig::default(), 4);
        state.apply_success(result(0.25, 2, 0.5));
        state.record_error(ServiceError::UnsupportedMode(3));

        assert_eq!(state.latest_measurement().unwrap().measurement.offset, 0.25);
        assert_eq!(state.health_status(), HealthStatus::Unavailable);
        assert!(matches!(
            state.connection_error(),
            Some(ServiceError::UnsupportedMode(3))
        ));
    }

    #[test]
    fn history_is_bounded_and_contains_successful_offsets() {
        let mut state = ApplicationState::new(AppConfig::default(), 2);
        state.apply_success(result(1.0, 1, 0.1));
        state.apply_success(result(2.0, 1, 0.1));
        state.apply_success(result(3.0, 1, 0.1));

        assert_eq!(
            state.history().samples().copied().collect::<Vec<_>>(),
            vec![2.0, 3.0]
        );
    }

    #[test]
    fn invalid_server_selection_is_atomic_and_valid_selection_resets_results() {
        let mut state = ApplicationState::new(config_with_servers(), 4);
        state.apply_success(result(0.25, 2, 0.5));
        let history_before = state.history().samples().copied().collect::<Vec<_>>();

        assert!(state.set_active_server(Some(99)).is_err());
        assert_eq!(state.config().active_server_index(), Some(0));
        assert_eq!(
            state.history().samples().copied().collect::<Vec<_>>(),
            history_before
        );
        assert!(state.latest_measurement().is_some());

        state.set_active_server(Some(1)).unwrap();
        assert_eq!(state.active_server().unwrap().name(), "two");
        assert!(state.latest_measurement().is_none());
        assert!(state.history().is_empty());
        assert_eq!(state.health_status(), HealthStatus::Unavailable);
    }

    #[allow(dead_code)]
    fn _timestamp_type_is_used(_: NtpTimestamp) {}
}
