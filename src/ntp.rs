//! NTPv4 packet primitives.
//!
//! This module deliberately contains no networking. Transport and clock
//! measurement code can build on these validated packet types independently.

use std::fmt;

pub const NTP_PACKET_LEN: usize = 48;
pub const NTP_VERSION: u8 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NtpTimestamp {
    pub seconds: u32,
    pub fraction: u32,
}

impl NtpTimestamp {
    pub const ZERO: Self = Self {
        seconds: 0,
        fraction: 0,
    };

    pub fn from_network_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 8 {
            return None;
        }
        Some(Self {
            seconds: u32::from_be_bytes(bytes[0..4].try_into().ok()?),
            fraction: u32::from_be_bytes(bytes[4..8].try_into().ok()?),
        })
    }

    pub fn write_network_bytes(self, output: &mut [u8]) -> bool {
        if output.len() < 8 {
            return false;
        }
        output[..4].copy_from_slice(&self.seconds.to_be_bytes());
        output[4..8].copy_from_slice(&self.fraction.to_be_bytes());
        true
    }

    pub fn as_seconds(self) -> f64 {
        self.seconds as f64 + self.fraction as f64 / u32::MAX as f64 / 2.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NtpHeader {
    pub leap_indicator: u8,
    pub version: u8,
    pub mode: u8,
    pub stratum: u8,
    pub poll_exponent: i8,
    pub precision_exponent: i8,
    pub root_delay: u32,
    pub root_dispersion: u32,
    pub reference_id: [u8; 4],
    pub reference_timestamp: NtpTimestamp,
    pub originate_timestamp: NtpTimestamp,
    pub receive_timestamp: NtpTimestamp,
    pub transmit_timestamp: NtpTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PacketError {
    TooShort { actual: usize },
    InvalidVersion(u8),
    InvalidMode(u8),
}

impl fmt::Display for PacketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort { actual } => write!(f, "NTP packet is {actual} bytes; expected 48"),
            Self::InvalidVersion(version) => write!(f, "unsupported NTP version {version}"),
            Self::InvalidMode(mode) => write!(f, "invalid NTP mode {mode}"),
        }
    }
}

impl std::error::Error for PacketError {}

pub fn client_request() -> [u8; NTP_PACKET_LEN] {
    // LI=0, VN=4, mode=3 (client): 00_100_011.
    [0x23; 1]
        .into_iter()
        .chain(std::iter::repeat_n(0, NTP_PACKET_LEN - 1))
        .collect::<Vec<_>>()
        .try_into()
        .expect("NTP packet length is fixed")
}

pub fn parse_header(packet: &[u8]) -> Result<NtpHeader, PacketError> {
    if packet.len() < NTP_PACKET_LEN {
        return Err(PacketError::TooShort {
            actual: packet.len(),
        });
    }

    let first = packet[0];
    let version = (first >> 3) & 0b111;
    let mode = first & 0b111;
    if !(3..=4).contains(&version) {
        return Err(PacketError::InvalidVersion(version));
    }
    if mode == 0 || mode > 6 {
        return Err(PacketError::InvalidMode(mode));
    }

    Ok(NtpHeader {
        leap_indicator: first >> 6,
        version,
        mode,
        stratum: packet[1],
        poll_exponent: packet[2] as i8,
        precision_exponent: packet[3] as i8,
        root_delay: u32::from_be_bytes(packet[4..8].try_into().unwrap()),
        root_dispersion: u32::from_be_bytes(packet[8..12].try_into().unwrap()),
        reference_id: packet[12..16].try_into().unwrap(),
        reference_timestamp: NtpTimestamp::from_network_bytes(&packet[16..24]).unwrap(),
        originate_timestamp: NtpTimestamp::from_network_bytes(&packet[24..32]).unwrap(),
        receive_timestamp: NtpTimestamp::from_network_bytes(&packet[32..40]).unwrap(),
        transmit_timestamp: NtpTimestamp::from_network_bytes(&packet[40..48]).unwrap(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_an_ntp_v4_client_request() {
        let packet = client_request();
        assert_eq!(packet.len(), NTP_PACKET_LEN);
        assert_eq!(packet[0], 0x23);
        assert!(packet[1..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn parses_header_and_timestamps() {
        let mut packet = client_request();
        packet[0] = 0x24; // LI=0, VN=4, server mode=4.
        packet[1] = 2;
        packet[40..44].copy_from_slice(&1u32.to_be_bytes());
        packet[44..48].copy_from_slice(&u32::MAX.to_be_bytes());

        let header = parse_header(&packet).unwrap();
        assert_eq!(header.version, 4);
        assert_eq!(header.mode, 4);
        assert_eq!(header.stratum, 2);
        assert_eq!(header.transmit_timestamp.seconds, 1);
        assert_eq!(header.transmit_timestamp.fraction, u32::MAX);
    }

    #[test]
    fn rejects_short_and_invalid_packets() {
        assert_eq!(
            parse_header(&[0; 10]),
            Err(PacketError::TooShort { actual: 10 })
        );

        let mut packet = client_request();
        packet[0] = 0x08; // version 1, client mode.
        assert_eq!(parse_header(&packet), Err(PacketError::InvalidVersion(1)));

        packet[0] = 0x20; // version 4, reserved mode 0.
        assert_eq!(parse_header(&packet), Err(PacketError::InvalidMode(0)));
    }
}
