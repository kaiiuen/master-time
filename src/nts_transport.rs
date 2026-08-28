//! Boundary for a future Network Time Security (NTS) transport.
//!
//! This module deliberately does not implement NTS-KE, packet protection, or
//! cryptography. Plain NTP remains owned by [`crate::transport`]. A vetted
//! implementation can later implement [`NtsTransportBackend`] without
//! changing the configuration and policy boundary here.

use std::fmt;
use std::time::Duration;

use crate::nts::{
    EndpointSecurityMode, EndpointSecurityReport, EndpointSecurityStatus, NtsKeEndpoint,
    NtsUnsupportedFeature,
};

/// Default upper bound that a future NTS-KE exchange may use.
pub const DEFAULT_NTS_TIMEOUT: Duration = Duration::from_secs(5);

/// Typed configuration for an NTS transport attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NtsTransportConfig {
    endpoint: NtsKeEndpoint,
    mode: EndpointSecurityMode,
    timeout: Duration,
}

impl NtsTransportConfig {
    /// Build an NTS configuration with the standard NTS-KE timeout.
    pub fn new(
        endpoint: NtsKeEndpoint,
        mode: EndpointSecurityMode,
    ) -> Result<Self, NtsTransportConfigError> {
        Self::with_timeout(endpoint, mode, DEFAULT_NTS_TIMEOUT)
    }

    /// Build an NTS configuration with an explicit, non-zero timeout.
    pub fn with_timeout(
        endpoint: NtsKeEndpoint,
        mode: EndpointSecurityMode,
        timeout: Duration,
    ) -> Result<Self, NtsTransportConfigError> {
        if timeout.is_zero() {
            return Err(NtsTransportConfigError::ZeroTimeout);
        }

        Ok(Self {
            endpoint,
            mode,
            timeout,
        })
    }

    pub const fn endpoint(&self) -> &NtsKeEndpoint {
        &self.endpoint
    }

    pub const fn mode(&self) -> EndpointSecurityMode {
        self.mode
    }

    pub const fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Report capability without claiming that an NTS exchange occurred.
    pub fn security_report(&self) -> EndpointSecurityReport {
        EndpointSecurityReport::for_mode(self.mode)
    }

    /// List the exact RFC 8915 components unavailable to this boundary.
    pub fn unsupported_nts_features(&self) -> &'static [NtsUnsupportedFeature] {
        self.security_report().status.unsupported_nts_features()
    }
}

/// Configuration errors that can be identified before any network operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NtsTransportConfigError {
    /// A zero timeout cannot bound a future network operation.
    ZeroTimeout,
}

impl fmt::Display for NtsTransportConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroTimeout => f.write_str("NTS timeout must be greater than zero"),
        }
    }
}

impl std::error::Error for NtsTransportConfigError {}

/// Why this boundary cannot execute a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NtsTransportError {
    /// NTS-KE, AEAD packet protection, cookies, authenticated extension
    /// fields, and replay protection are intentionally not implemented here.
    NotImplemented,
    /// The selected policy belongs to another transport boundary.
    Unsupported(UnsupportedNtsPolicy),
}

impl fmt::Display for NtsTransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotImplemented => f.write_str("NTS transport is not implemented"),
            Self::Unsupported(policy) => policy.fmt(f),
        }
    }
}

impl std::error::Error for NtsTransportError {}

/// Policies that this NTS-only boundary refuses to execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedNtsPolicy {
    /// Plain UDP must be executed through the ordinary NTP transport.
    PlainUdp,
}

impl fmt::Display for UnsupportedNtsPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PlainUdp => f.write_str("plain UDP is not an NTS transport"),
        }
    }
}

/// Future cryptographic/network implementation boundary.
///
/// Implementors are expected to perform NTS-KE and protected NTP using a
/// vetted crypto stack. This trait intentionally exposes no placeholder keys,
/// authentication tags, or unauthenticated fallback behavior.
pub trait NtsTransportBackend {
    type Error;

    fn exchange(&self, config: &NtsTransportConfig, request: &[u8])
    -> Result<Vec<u8>, Self::Error>;
}

/// NTS boundary used until a vetted backend is available.
#[derive(Debug, Default, Clone, Copy)]
pub struct NtsTransportBoundary;

impl NtsTransportBoundary {
    pub const fn new() -> Self {
        Self
    }

    /// Refuse execution explicitly; this method never performs plain UDP or
    /// unauthenticated NTS-like traffic. In particular, it does not send the
    /// request to the NTS-KE endpoint or silently downgrade an NTS policy.
    pub fn execute(
        &self,
        config: &NtsTransportConfig,
        _request: &[u8],
    ) -> Result<Vec<u8>, NtsTransportError> {
        match config.security_report().status {
            EndpointSecurityStatus::PlainUdpSupported => Err(NtsTransportError::Unsupported(
                UnsupportedNtsPolicy::PlainUdp,
            )),
            EndpointSecurityStatus::NtsUnsupported => Err(NtsTransportError::NotImplemented),
            EndpointSecurityStatus::InvalidEndpoint => {
                // NtsTransportConfig owns a validated endpoint, so this arm
                // documents the capability contract if that type grows later.
                Err(NtsTransportError::NotImplemented)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(mode: EndpointSecurityMode) -> NtsTransportConfig {
        NtsTransportConfig::new("time.example.test:4460".parse().unwrap(), mode).unwrap()
    }

    #[test]
    fn preserves_policy_in_capability_report() {
        let plain = config(EndpointSecurityMode::PlainUdp);
        assert_eq!(plain.security_report().mode, EndpointSecurityMode::PlainUdp);
        assert_eq!(
            plain.security_report().status,
            EndpointSecurityStatus::PlainUdpSupported
        );

        let required = config(EndpointSecurityMode::NtsRequired);
        assert_eq!(
            required.security_report().mode,
            EndpointSecurityMode::NtsRequired
        );
        assert_eq!(
            required.security_report().status,
            EndpointSecurityStatus::NtsUnsupported
        );
    }

    #[test]
    fn rejects_zero_timeout_in_typed_configuration() {
        let result = NtsTransportConfig::with_timeout(
            "time.example.test:4460".parse().unwrap(),
            EndpointSecurityMode::NtsRequired,
            Duration::ZERO,
        );
        assert_eq!(result, Err(NtsTransportConfigError::ZeroTimeout));
    }

    #[test]
    fn refuses_plain_udp_in_the_nts_boundary() {
        let result =
            NtsTransportBoundary::new().execute(&config(EndpointSecurityMode::PlainUdp), &[]);
        assert_eq!(
            result,
            Err(NtsTransportError::Unsupported(
                UnsupportedNtsPolicy::PlainUdp
            ))
        );
    }

    #[test]
    fn reports_unimplemented_execution_for_nts_policies() {
        for mode in [
            EndpointSecurityMode::NtsRequired,
            EndpointSecurityMode::NtsPreferred,
        ] {
            let configuration = config(mode);
            let result = NtsTransportBoundary::new().execute(&configuration, &[0; 48]);
            assert_eq!(result, Err(NtsTransportError::NotImplemented));
            assert_eq!(
                configuration.unsupported_nts_features().len(),
                5,
                "NTS must not be reported as partially implemented"
            );
        }
    }
}
