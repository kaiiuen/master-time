//! UDP transport for NTP queries.
//!
//! This module owns networking and timing only. NTP packet construction and
//! validation remain in [`crate::ntp`], which keeps the transport easy to test
//! with a local UDP server.

use std::fmt;
use std::io;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

use crate::ntp::{self, PacketError};

/// The well-known UDP port used by NTP.
pub const NTP_PORT: u16 = 123;

/// Default upper bound for waiting for an NTP response.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// A successful NTP exchange.
#[derive(Debug)]
pub struct NtpResponse {
    /// The complete response datagram, including the NTP header.
    pub packet: Vec<u8>,
    /// The resolved address used for this exchange.
    pub server: SocketAddr,
    /// Monotonic instant immediately before sending the request.
    pub sent_at: Instant,
    /// Monotonic instant when the response was received.
    pub received_at: Instant,
}

impl NtpResponse {
    /// Elapsed time between sending the request and receiving the response.
    pub fn round_trip_time(&self) -> Duration {
        self.received_at.saturating_duration_since(self.sent_at)
    }

    /// Parse the response as an NTP header.
    pub fn header(&self) -> Result<ntp::NtpHeader, PacketError> {
        ntp::parse_header(&self.packet)
    }
}

/// Errors produced while resolving or performing an NTP exchange.
#[derive(Debug)]
pub enum TransportError {
    /// The server name or address could not be resolved.
    Resolve { server: String, source: io::Error },
    /// The supplied timeout is zero and cannot bound a read.
    InvalidTimeout,
    /// No address was returned by name resolution.
    NoAddresses { server: String },
    /// The local UDP socket could not be created.
    Bind(io::Error),
    /// Connecting the UDP socket to the resolved peer failed.
    Connect {
        server: SocketAddr,
        source: io::Error,
    },
    /// Sending the NTP request failed.
    Send(io::Error),
    /// Receiving the NTP response failed, including timeout expiry.
    Receive(io::Error),
    /// The received datagram was not a valid NTP packet.
    InvalidResponse(PacketError),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Resolve { server, source } => {
                write!(f, "could not resolve NTP server {server:?}: {source}")
            }
            Self::InvalidTimeout => write!(f, "NTP timeout must be greater than zero"),
            Self::NoAddresses { server } => {
                write!(f, "NTP server {server:?} resolved to no addresses")
            }
            Self::Bind(source) => write!(f, "could not bind the NTP UDP socket: {source}"),
            Self::Connect { server, source } => {
                write!(f, "could not connect to NTP server {server}: {source}")
            }
            Self::Send(source) => write!(f, "could not send NTP request: {source}"),
            Self::Receive(source) => write!(f, "could not receive NTP response: {source}"),
            Self::InvalidResponse(source) => write!(f, "invalid NTP response: {source}"),
        }
    }
}

impl std::error::Error for TransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Resolve { source, .. }
            | Self::Bind(source)
            | Self::Send(source)
            | Self::Receive(source) => Some(source),
            Self::Connect { source, .. } => Some(source),
            Self::InvalidTimeout | Self::NoAddresses { .. } | Self::InvalidResponse(_) => None,
        }
    }
}

/// A configurable, synchronous UDP NTP client.
#[derive(Debug, Clone, Copy)]
pub struct NtpTransport {
    timeout: Duration,
}

impl Default for NtpTransport {
    fn default() -> Self {
        Self::new(DEFAULT_TIMEOUT)
    }
}

impl NtpTransport {
    /// Create a transport whose receive operation is bounded by `timeout`.
    pub const fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// Return the configured response timeout.
    pub const fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Query `server` on the standard NTP port.
    pub fn query(&self, server: &str) -> Result<NtpResponse, TransportError> {
        self.query_with_port(server, NTP_PORT)
    }

    /// Query `server` on an explicit port. Useful for local test servers.
    pub fn query_with_port(&self, server: &str, port: u16) -> Result<NtpResponse, TransportError> {
        let mut addresses =
            (server, port)
                .to_socket_addrs()
                .map_err(|source| TransportError::Resolve {
                    server: server.to_owned(),
                    source,
                })?;
        let address = addresses
            .next()
            .ok_or_else(|| TransportError::NoAddresses {
                server: server.to_owned(),
            })?;
        self.query_addr(address)
    }

    /// Query an already-resolved address without performing DNS resolution.
    pub fn query_addr(&self, server: SocketAddr) -> Result<NtpResponse, TransportError> {
        if self.timeout.is_zero() {
            return Err(TransportError::InvalidTimeout);
        }

        let bind_addr = if server.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        };
        let socket = UdpSocket::bind(bind_addr).map_err(TransportError::Bind)?;
        socket
            .set_read_timeout(Some(self.timeout))
            .map_err(TransportError::Receive)?;
        socket
            .connect(server)
            .map_err(|source| TransportError::Connect { server, source })?;

        let request = ntp::client_request();
        let sent_at = Instant::now();
        socket.send(&request).map_err(TransportError::Send)?;

        let mut packet = [0u8; 2048];
        let length = socket.recv(&mut packet).map_err(TransportError::Receive)?;
        let received_at = Instant::now();
        let packet = packet[..length].to_vec();
        ntp::parse_header(&packet).map_err(TransportError::InvalidResponse)?;

        Ok(NtpResponse {
            packet,
            server,
            sent_at,
            received_at,
        })
    }
}

/// Query an NTP server using the default timeout and port.
pub fn query(server: &str) -> Result<NtpResponse, TransportError> {
    NtpTransport::default().query(server)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn exchanges_a_request_with_a_local_server() {
        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        let address = server.local_addr().unwrap();
        let worker = thread::spawn(move || {
            let mut request = [0; ntp::NTP_PACKET_LEN];
            let (length, peer) = server.recv_from(&mut request).unwrap();
            assert_eq!(length, ntp::NTP_PACKET_LEN);
            assert_eq!(request[0], 0x23);

            let mut response = ntp::client_request();
            response[0] = 0x24; // NTPv4 server mode.
            server.send_to(&response, peer).unwrap();
        });

        let response = NtpTransport::new(Duration::from_secs(1))
            .query_addr(address)
            .unwrap();
        worker.join().unwrap();

        assert_eq!(response.packet.len(), ntp::NTP_PACKET_LEN);
        assert!(response.received_at >= response.sent_at);
        assert_eq!(response.header().unwrap().mode, 4);
    }

    #[test]
    fn rejects_a_zero_timeout_before_binding() {
        let result = NtpTransport::new(Duration::ZERO).query_addr("127.0.0.1:123".parse().unwrap());
        assert!(matches!(result, Err(TransportError::InvalidTimeout)));
    }
}
