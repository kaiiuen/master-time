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
pub use clock_display::{ClockDisplayModel, DisplayMode, HourFormat, TimeZone};
pub use global_servers::{GlobalServerCatalog, GlobalServerEntry};

pub use service::{
    MeasurementResult, NtpMeasurementService, ServiceError, assemble_result, measure,
    system_time_to_ntp_timestamp,
};

pub mod health;

pub use health::{HealthInput, HealthStatus, LeapIndicator, evaluate};

pub mod clock_display;
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
pub mod sync_policy;
pub mod time_action;

pub use storage::{StorageError, load, load_config, save, save_config};
pub use sync_policy::{DEFAULT_MAX_CORRECTION_OFFSET, SyncDisposition, SyncPolicy};
pub use time_action::{
    ApprovedCorrection, CorrectionPreview, CorrectionRefusal, CorrectionRequest, CorrectionResult,
    TimeAction,
};

pub mod localization;

pub use localization::{English, Key, MISSING_TRANSLATION};

pub mod persistence;

pub use persistence::{Persistence, PersistenceError, PersistenceManager};

pub mod chart;

pub use chart::{
    ChartRenderer, PlotGeometry, normalize_value, normalized_to_screen, plot_geometry, show,
    zero_line_y,
};

pub mod translations;

pub use translations::{
    Catalog, Key as TranslationKey, Language as TranslationLanguage, TranslationCatalog,
};

pub mod recovery;

pub use recovery::{DEFAULT_INITIAL_DELAY, DEFAULT_MAX_DELAY, RecoveryDecision, RetryPolicy};

pub mod nts;

pub use nts::{
    EndpointSecurityMode, EndpointSecurityReport, EndpointSecurityStatus, NtsKeEndpoint,
    NtsKeEndpointError, NtsTransport,
};

pub mod system_tray;

pub use system_tray::{
    HIDE_LABEL, MENU_ITEMS, QUIT_LABEL, SHOW_LABEL, START_POLLING_LABEL, STOP_POLLING_LABEL,
    SystemTrayState, TrayAction, TrayCommand, TrayEvent, TrayMenuItem, menu_items,
};

pub mod nts_transport;

pub use nts_transport::{
    DEFAULT_NTS_TIMEOUT, NtsTransportBackend, NtsTransportBoundary, NtsTransportConfig,
    NtsTransportConfigError, NtsTransportError, UnsupportedNtsPolicy,
};

pub mod platform_time;

pub use platform_time::{
    AppliedCorrection, CorrectionDryRun, PlatformTimeAdapter, PlatformTimeError,
};

pub mod failover;

pub use failover::FailoverCoordinator;

pub mod network_stats;

pub use network_stats::{
    NetworkStatistics, NetworkStatisticsAccumulator, NetworkStatisticsError, NetworkStats,
};

pub mod notifications;

pub use notifications::{
    Clock as NotificationClock, Notification, NotificationCenter, NotificationKind, Severity,
    SystemClock as NotificationSystemClock,
};

pub mod polling_failover;

pub use polling_failover::{PollingOrchestrator, PollingTransition};

pub mod network_view;

pub use network_view::{MetricValue, NetworkMetric, NetworkMetricRow, NetworkViewModel};

pub mod diagnostic_export;

pub use diagnostic_export::{DiagnosticSnapshot, ExportError, ServerInfo};

pub mod system_tray_backend;

pub use system_tray_backend::{
    SystemTrayBackend, TrayBackendAvailability, TrayBackendError, TrayBackendLifecycle,
    command_for_menu_id,
};
