//! Synchronous NTP measurement orchestration.
//!
//! The service owns the small amount of policy needed to turn a transport
//! response into a four-timestamp measurement. Networking remains in
//! [`crate::transport`], packet parsing in [`crate::ntp`], and calculations in
//! [`crate::measurement`].

use std::fmt;
use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::measurement::{self, FourTimestamps, Measurement, TimestampName};
use crate::ntp::{self, NtpHeader, PacketError};
use crate::servers::ServerProfile;
use crate::transport::{NtpTransport, TransportError};

/// The number of seconds between the Unix and NTP epochs.
const NTP_UNIX_OFFSET: u64 = 2_208_988_800;

/// A complete result from one synchronous NTP exchange.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeasurementResult {
    /// The resolved address used by the transport.
    pub server: SocketAddr,
    /// The parsed response header, retained for callers that need raw fields.
    pub header: NtpHeader,
    /// The four timestamps used for the calculation.
    pub timestamps: FourTimestamps,
    /// Offset, round-trip delay, and root distance, all in seconds.
    pub measurement: Measurement,
}

/// Errors returned by the measurement service.
#[derive(Debug)]
pub enum ServiceError {
    Transport(TransportError),
    Packet(PacketError),
    Measurement(measurement::MeasurementError),
    UnsupportedMode(u8),
    MissingTimestamp(TimestampName),
    ClockBeforeNtpEpoch,
    TimestampOutOfRange,
}

impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(source) => write!(f, "NTP transport failed: {source}"),
            Self::Packet(source) => write!(f, "invalid NTP response: {source}"),
            Self::Measurement(source) => write!(f, "could not calculate NTP measurement: {source}"),
            Self::UnsupportedMode(mode) => {
                write!(
                    f,
                    "unsupported NTP response mode {mode}; expected server mode"
                )
            }
            Self::MissingTimestamp(name) => write!(f, "missing {name} timestamp"),
            Self::ClockBeforeNtpEpoch => f.write_str("local clock is before the NTP epoch"),
            Self::TimestampOutOfRange => {
                f.write_str("local time cannot be represented as an NTP timestamp")
            }
        }
    }
}

impl std::error::Error for ServiceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Transport(source) => Some(source),
            Self::Packet(source) => Some(source),
            Self::Measurement(source) => Some(source),
            Self::UnsupportedMode(_)
            | Self::MissingTimestamp(_)
            | Self::ClockBeforeNtpEpoch
            | Self::TimestampOutOfRange => None,
        }
    }
}

impl From<TransportError> for ServiceError {
    fn from(error: TransportError) -> Self {
        Self::Transport(error)
    }
}

impl From<PacketError> for ServiceError {
    fn from(error: PacketError) -> Self {
        Self::Packet(error)
    }
}

/// Converts a system time to an NTP timestamp (seconds since 1900).
pub fn system_time_to_ntp_timestamp(time: SystemTime) -> Result<ntp::NtpTimestamp, ServiceError> {
    let elapsed = time
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ServiceError::ClockBeforeNtpEpoch)?;
    let seconds = elapsed
        .as_secs()
        .checked_add(NTP_UNIX_OFFSET)
        .ok_or(ServiceError::TimestampOutOfRange)?;
    let seconds = u32::try_from(seconds).map_err(|_| ServiceError::TimestampOutOfRange)?;
    let fraction = ((u64::from(elapsed.subsec_nanos()) << 32) / 1_000_000_000) as u32;
    Ok(ntp::NtpTimestamp { seconds, fraction })
}

/// Assembles and calculates a result from a parsed response and local times.
///
/// This function is separate from [`NtpMeasurementService::measure`] so result
/// assembly can be tested without DNS or network access.
pub fn assemble_result(
    server: SocketAddr,
    header: NtpHeader,
    sent_at: SystemTime,
    received_at: SystemTime,
) -> Result<MeasurementResult, ServiceError> {
    if header.mode != 4 {
        return Err(ServiceError::UnsupportedMode(header.mode));
    }

    let originate = system_time_to_ntp_timestamp(sent_at)?;
    let destination = system_time_to_ntp_timestamp(received_at)?;
    let receive = nonzero_timestamp(header.receive_timestamp)
        .ok_or(ServiceError::MissingTimestamp(TimestampName::Receive))?;
    let transmit = nonzero_timestamp(header.transmit_timestamp)
        .ok_or(ServiceError::MissingTimestamp(TimestampName::Transmit))?;

    let timestamps = FourTimestamps::complete(
        measurement::NtpTimestamp::new(originate.seconds, originate.fraction),
        measurement::NtpTimestamp::new(receive.seconds, receive.fraction),
        measurement::NtpTimestamp::new(transmit.seconds, transmit.fraction),
        measurement::NtpTimestamp::new(destination.seconds, destination.fraction),
    );
    let measurement = measurement::calculate(
        timestamps,
        header.root_delay as f64 / 65_536.0,
        header.root_dispersion as f64 / 65_536.0,
    )
    .map_err(ServiceError::Measurement)?;

    Ok(MeasurementResult {
        server,
        header,
        timestamps,
        measurement,
    })
}

fn nonzero_timestamp(timestamp: ntp::NtpTimestamp) -> Option<ntp::NtpTimestamp> {
    (timestamp != ntp::NtpTimestamp::ZERO).then_some(timestamp)
}

/// A synchronous service using the supplied NTP transport.
#[derive(Debug, Clone, Copy)]
pub struct NtpMeasurementService {
    transport: NtpTransport,
}

impl NtpMeasurementService {
    pub const fn new(transport: NtpTransport) -> Self {
        Self { transport }
    }

    pub const fn transport(&self) -> NtpTransport {
        self.transport
    }

    /// Queries the profile hostname and calculates one measurement.
    pub fn measure(&self, profile: &ServerProfile) -> Result<MeasurementResult, ServiceError> {
        let sent_at = SystemTime::now();
        let response = self.transport.query(profile.hostname())?;
        let received_at = SystemTime::now();
        let header = response.header()?;
        assemble_result(response.server, header, sent_at, received_at)
    }
}

/// Queries `profile` once using `transport`.
pub fn measure(
    profile: &ServerProfile,
    transport: NtpTransport,
) -> Result<MeasurementResult, ServiceError> {
    NtpMeasurementService::new(transport).measure(profile)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn converts_unix_epoch_to_ntp_epoch() {
        let timestamp = system_time_to_ntp_timestamp(UNIX_EPOCH).unwrap();
        assert_eq!(
            timestamp,
            ntp::NtpTimestamp {
                seconds: NTP_UNIX_OFFSET as u32,
                fraction: 0
            }
        );
    }

    #[test]
    fn rejects_missing_server_timestamps() {
        let header = NtpHeader {
            leap_indicator: 0,
            version: 4,
            mode: 4,
            stratum: 1,
            poll_exponent: 0,
            precision_exponent: 0,
            root_delay: 0,
            root_dispersion: 0,
            reference_id: [0; 4],
            reference_timestamp: ntp::NtpTimestamp::ZERO,
            originate_timestamp: ntp::NtpTimestamp::ZERO,
            receive_timestamp: ntp::NtpTimestamp::ZERO,
            transmit_timestamp: ntp::NtpTimestamp {
                seconds: 1,
                fraction: 0,
            },
        };
        let result = assemble_result(
            "127.0.0.1:123".parse().unwrap(),
            header,
            UNIX_EPOCH,
            UNIX_EPOCH + Duration::from_secs(1),
        );
        assert!(matches!(
            result,
            Err(ServiceError::MissingTimestamp(TimestampName::Receive))
        ));
    }
}
