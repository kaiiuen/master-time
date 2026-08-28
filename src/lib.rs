//! Testable building blocks for the Master Time application.

pub mod measurement;
pub mod ntp;
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
