//! Testable building blocks for the Master Time application.

pub mod calibration;
pub mod global_servers;
pub mod history_view;
pub mod measurement;
pub mod ntp;
pub mod server_manager;
pub mod servers;
pub mod settings;
pub mod transport;

pub mod service;

pub use calibration::{
    Calibration, CalibrationResult, CalibrationView, Clock, ClockSample, SystemClock,
    next_minute_boundary,
};
pub use global_servers::{GlobalServerCatalog, GlobalServerEntry};

pub use service::{
    MeasurementResult, NtpMeasurementService, ServiceError, assemble_result, measure,
    system_time_to_ntp_timestamp,
};

pub mod health;

pub use health::{HealthInput, HealthStatus, LeapIndicator, evaluate};

pub mod config;
pub mod diagnostics_view;

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

pub use diagnostics_view::{DiagnosticsRow, DiagnosticsView};
pub use history_view::{ChartModel, NormalizedPoint, ValueRange};
pub use settings::{Language, LocalSettings, SettingsDraft, SettingsError, SettingsModel, Theme};

pub mod storage;

pub use storage::{StorageError, load, load_config, save, save_config};

pub mod localization;

pub use localization::{English, Key, MISSING_TRANSLATION};
