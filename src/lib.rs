//! Testable building blocks for the Master Time application.

pub mod measurement;
pub mod ntp;
pub mod server_manager;
pub mod servers;
pub mod transport;

pub mod service;

pub use service::{
    MeasurementResult, NtpMeasurementService, ServiceError, assemble_result, measure,
    system_time_to_ntp_timestamp,
};

pub mod health;

pub use health::{HealthInput, HealthStatus, LeapIndicator, evaluate};

pub mod config;

pub use config::{
    AppConfig, ConfigError, PollingPreferences, ServerProfile as ConfigServerProfile,
};

pub mod platform;

pub use platform::{DiagnosticsCollector, DiagnosticsSnapshot, collect_diagnostics};

pub mod polling;

pub use polling::{PollEvent, PollingError, PollingWorker};

pub mod state;

pub use state::{AppState, ApplicationState, DEFAULT_HISTORY_CAPACITY, PollingState};

pub use server_manager::{ServerManager, ServerManagerError};

pub mod storage;

pub use storage::{StorageError, load, load_config, save, save_config};

pub mod localization;

pub use localization::{English, Key, MISSING_TRANSLATION};
