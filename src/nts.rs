//! Models Network Time Security (NTS) capability and policy.
//!
//! This module intentionally does not implement NTS or NTS-KE. It describes
//! what the application can report today and leaves the wire protocol behind
//! [`NtsTransport`], so an eventual implementation can be added without
//! making an ordinary UDP exchange look like an NTS exchange.
//!
//! NTS is not just an NTP extension field: RFC 8915 requires a TLS-protected
//! NTS-KE exchange, negotiated AEAD keys, cookies, authenticated NTP extension
//! fields, and replay protection. None of those operations are implemented in
//! this crate, and plain UDP must never be reported as an NTS result.
//!
//! ## Dependency review (2026-08-28)
//!
//! The crates.io candidates reviewed were `rtime-nts` 0.15.0, `rusty_time-nts`
//! 0.1.7, `rkik-nts` 1.2.0, and `ntp_usg-client` 5.0.0. They advertise RFC
//! 8915 coverage, but none is accepted here as a mature compatible dependency:
//! `rtime-nts` and `rusty_time-nts` were published in 2026 and have very low
//! download counts (124 and 101 total); `rkik-nts` is newer (created in 2025)
//! with 20,603 total downloads. `ntp_usg-client` is a larger
//! general client whose NTS API/runtime and Rust 1.93 requirement do not match
//! this synchronous, existing transport boundary. `rtime-nts` also targets a
//! different daemon, and `rusty_time-nts` requires Rust 1.95. These signals are
//! insufficient for importing security-sensitive protocol code without a
//! dedicated review and interoperability test plan.

use std::fmt;
use std::str::FromStr;

/// The security policy requested for an endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointSecurityMode {
    /// Use ordinary NTP over UDP. This does not provide NTS security.
    PlainUdp,
    /// Use NTS and refuse an endpoint if NTS cannot be established.
    NtsRequired,
    /// Prefer NTS, but allow an explicit fallback to ordinary UDP.
    NtsPreferred,
}

impl EndpointSecurityMode {
    /// Whether this policy requires an NTS implementation.
    pub const fn requires_nts(self) -> bool {
        matches!(self, Self::NtsRequired | Self::NtsPreferred)
    }

    /// Whether ordinary UDP is an allowed fallback for this policy.
    pub const fn allows_plain_udp(self) -> bool {
        matches!(self, Self::PlainUdp | Self::NtsPreferred)
    }
}

/// NTS protocol components deliberately not implemented by this crate.
///
/// Keeping these as named capabilities prevents a future caller from treating
/// an NTS-shaped packet as secure when only some of the protocol was added.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NtsUnsupportedFeature {
    /// RFC 8915 NTS-KE over TLS 1.3, including certificate verification.
    NtsKeyEstablishment,
    /// Negotiated AEAD key handling for protected NTP packets.
    PacketAead,
    /// NTS cookies and their lifecycle across NTP exchanges.
    Cookies,
    /// NTS authenticator and other required NTP extension fields.
    AuthenticatedExtensionFields,
    /// Validation of unique request identifiers and replay resistance.
    ReplayProtection,
}

/// Every RFC 8915 component that is absent from the current implementation.
pub const NTS_UNSUPPORTED_FEATURES: &[NtsUnsupportedFeature] = &[
    NtsUnsupportedFeature::NtsKeyEstablishment,
    NtsUnsupportedFeature::PacketAead,
    NtsUnsupportedFeature::Cookies,
    NtsUnsupportedFeature::AuthenticatedExtensionFields,
    NtsUnsupportedFeature::ReplayProtection,
];

impl fmt::Display for NtsUnsupportedFeature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NtsKeyEstablishment => "NTS-KE over TLS 1.3",
            Self::PacketAead => "NTS packet AEAD protection",
            Self::Cookies => "NTS cookies",
            Self::AuthenticatedExtensionFields => "NTS authenticated extension fields",
            Self::ReplayProtection => "NTS replay protection",
        })
    }
}

/// The currently reportable security capability of an endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointSecurityStatus {
    /// The endpoint can be queried using ordinary NTP over UDP only.
    PlainUdpSupported,
    /// NTS is not implemented; see [`NTS_UNSUPPORTED_FEATURES`].
    NtsUnsupported,
    /// The endpoint string is invalid and cannot be used.
    InvalidEndpoint,
}

impl EndpointSecurityStatus {
    pub const fn is_supported(self) -> bool {
        matches!(self, Self::PlainUdpSupported)
    }

    /// Whether this status represents an authenticated NTS exchange.
    ///
    /// This remains false for every current status because this crate has no
    /// NTS backend.
    pub const fn is_nts(self) -> bool {
        false
    }

    /// Return the exact RFC 8915 components missing from this implementation.
    pub const fn unsupported_nts_features(self) -> &'static [NtsUnsupportedFeature] {
        match self {
            Self::NtsUnsupported => NTS_UNSUPPORTED_FEATURES,
            Self::PlainUdpSupported | Self::InvalidEndpoint => &[],
        }
    }
}

impl fmt::Display for EndpointSecurityStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PlainUdpSupported => f.write_str("supported over plain UDP; NTS is not enabled"),
            Self::NtsUnsupported => f.write_str(
                "unsupported: NTS-KE, packet protection, cookies, authenticated extension fields, and replay protection are not implemented",
            ),
            Self::InvalidEndpoint => f.write_str("unsupported: invalid NTS-KE endpoint"),
        }
    }
}

/// A validated NTS Key Establishment endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NtsKeEndpoint {
    host: String,
    port: u16,
}

impl NtsKeEndpoint {
    /// The registered NTS-KE port.
    pub const DEFAULT_PORT: u16 = 4460;

    /// Validate and parse an endpoint in `host:port` form.
    pub fn parse(input: &str) -> Result<Self, NtsKeEndpointError> {
        input.parse()
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub const fn port(&self) -> u16 {
        self.port
    }
}

impl fmt::Display for NtsKeEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.host.contains(':') {
            write!(f, "[{}]:{}", self.host, self.port)
        } else {
            write!(f, "{}:{}", self.host, self.port)
        }
    }
}

impl FromStr for NtsKeEndpoint {
    type Err = NtsKeEndpointError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        if input.is_empty() {
            return Err(NtsKeEndpointError::Empty);
        }
        if input.chars().any(char::is_whitespace) {
            return Err(NtsKeEndpointError::Whitespace);
        }
        if input.contains("//") || input.contains('/') {
            return Err(NtsKeEndpointError::NotHostPort);
        }

        let (host, port) = if let Some(rest) = input.strip_prefix('[') {
            let (host, port) = rest
                .split_once("]:")
                .ok_or(NtsKeEndpointError::InvalidHostPort)?;
            if host.is_empty() || host.contains('[') || host.contains(']') {
                return Err(NtsKeEndpointError::InvalidHost);
            }
            (host, port)
        } else {
            let (host, port) = input
                .rsplit_once(':')
                .ok_or(NtsKeEndpointError::MissingPort)?;
            if host.is_empty() || host.contains(':') || host.contains('[') || host.contains(']') {
                return Err(NtsKeEndpointError::InvalidHost);
            }
            (host, port)
        };

        if port.is_empty() {
            return Err(NtsKeEndpointError::MissingPort);
        }
        let port = port
            .parse::<u16>()
            .map_err(|_| NtsKeEndpointError::InvalidPort)?;
        if port == 0 {
            return Err(NtsKeEndpointError::InvalidPort);
        }

        Ok(Self {
            host: host.to_owned(),
            port,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NtsKeEndpointError {
    Empty,
    Whitespace,
    NotHostPort,
    InvalidHostPort,
    InvalidHost,
    MissingPort,
    InvalidPort,
}

impl fmt::Display for NtsKeEndpointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "NTS-KE endpoint is empty",
            Self::Whitespace => "NTS-KE endpoint must not contain whitespace",
            Self::NotHostPort => "NTS-KE endpoint must be a host:port value",
            Self::InvalidHostPort => "bracketed IPv6 endpoint must use [host]:port form",
            Self::InvalidHost => "NTS-KE endpoint has an invalid host",
            Self::MissingPort => "NTS-KE endpoint is missing its port",
            Self::InvalidPort => "NTS-KE endpoint port must be between 1 and 65535",
        })
    }
}

impl std::error::Error for NtsKeEndpointError {}

/// Report the capability of a policy without implying that NTS was performed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointSecurityReport {
    pub mode: EndpointSecurityMode,
    pub status: EndpointSecurityStatus,
}

impl EndpointSecurityReport {
    pub fn for_mode(mode: EndpointSecurityMode) -> Self {
        let status = if mode == EndpointSecurityMode::PlainUdp {
            EndpointSecurityStatus::PlainUdpSupported
        } else {
            EndpointSecurityStatus::NtsUnsupported
        };
        Self { mode, status }
    }

    pub fn for_endpoint(mode: EndpointSecurityMode, endpoint: &str) -> Self {
        let status = if NtsKeEndpoint::parse(endpoint).is_err() {
            EndpointSecurityStatus::InvalidEndpoint
        } else if mode == EndpointSecurityMode::PlainUdp {
            EndpointSecurityStatus::PlainUdpSupported
        } else {
            EndpointSecurityStatus::NtsUnsupported
        };
        Self { mode, status }
    }
}

/// Boundary for a future NTS-KE/NTS transport implementation.
///
/// No implementation is provided here. In particular, this trait is not used
/// to turn the existing plain-UDP transport into an NTS transport.
pub trait NtsTransport {
    type Error;

    fn exchange(&self, endpoint: &NtsKeEndpoint, request: &[u8]) -> Result<Vec<u8>, Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_udp_is_explicitly_supported_without_claiming_nts() {
        let report = EndpointSecurityReport::for_mode(EndpointSecurityMode::PlainUdp);
        assert_eq!(report.status, EndpointSecurityStatus::PlainUdpSupported);
        assert!(!report.status.is_nts());
        assert_eq!(
            report.status.to_string(),
            "supported over plain UDP; NTS is not enabled"
        );
    }

    #[test]
    fn nts_modes_are_reported_as_unsupported() {
        for mode in [
            EndpointSecurityMode::NtsRequired,
            EndpointSecurityMode::NtsPreferred,
        ] {
            let status = EndpointSecurityReport::for_mode(mode).status;
            assert_eq!(status, EndpointSecurityStatus::NtsUnsupported);
            assert!(!status.is_nts());
            assert_eq!(status.unsupported_nts_features(), NTS_UNSUPPORTED_FEATURES);
        }
    }

    #[test]
    fn validates_host_port_and_ipv6_endpoints() {
        let endpoint: NtsKeEndpoint = "time.example.test:4460".parse().unwrap();
        assert_eq!(endpoint.host(), "time.example.test");
        assert_eq!(endpoint.port(), 4460);
        assert_eq!(endpoint.to_string(), "time.example.test:4460");

        let ipv6: NtsKeEndpoint = "[2001:db8::1]:4460".parse().unwrap();
        assert_eq!(ipv6.host(), "2001:db8::1");
        assert_eq!(ipv6.to_string(), "[2001:db8::1]:4460");
    }

    #[test]
    fn rejects_malformed_nts_ke_endpoints() {
        for input in [
            "",
            "time.example.test",
            "time.example.test:",
            "time.example.test:0",
            "time.example.test:65536",
            "https://time.example.test:4460",
            "2001:db8::1:4460",
            "[2001:db8::1]",
        ] {
            assert!(
                input.parse::<NtsKeEndpoint>().is_err(),
                "accepted {input:?}"
            );
        }
        assert_eq!(
            "time.example.test:4460"
                .parse::<NtsKeEndpoint>()
                .unwrap()
                .port(),
            NtsKeEndpoint::DEFAULT_PORT
        );
    }

    #[test]
    fn invalid_endpoint_is_not_reported_as_nts_support() {
        let report = EndpointSecurityReport::for_endpoint(
            EndpointSecurityMode::NtsRequired,
            "not-an-endpoint",
        );
        assert_eq!(report.status, EndpointSecurityStatus::InvalidEndpoint);
        assert!(!report.status.is_supported());
    }
}
