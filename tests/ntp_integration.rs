use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use master_time::measurement::NtpTimestamp as MeasurementTimestamp;
use master_time::ntp::{self, NTP_PACKET_LEN};
use master_time::servers::ServerProfile;
use master_time::service::NtpMeasurementService;
use master_time::transport::{NtpTransport, TransportError};

const TEST_TIMEOUT: Duration = Duration::from_millis(750);
const NTP_UNIX_OFFSET: u64 = 2_208_988_800;

static NTP_PORT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn current_ntp_timestamp() -> ntp::NtpTimestamp {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("test clock must be after the Unix epoch");
    ntp::NtpTimestamp {
        seconds: u32::try_from(elapsed.as_secs() + NTP_UNIX_OFFSET)
            .expect("test clock must fit in an NTP timestamp"),
        fraction: ((u64::from(elapsed.subsec_nanos()) << 32) / 1_000_000_000) as u32,
    }
}

fn write_timestamp(packet: &mut [u8], offset: usize, timestamp: ntp::NtpTimestamp) {
    assert!(timestamp.write_network_bytes(&mut packet[offset..offset + 8]));
}

fn spawn_fake_server(bind: SocketAddr) -> (SocketAddr, thread::JoinHandle<io::Result<()>>) {
    let socket = UdpSocket::bind(bind).expect("bind local fake NTP server");
    socket
        .set_read_timeout(Some(TEST_TIMEOUT))
        .expect("set fake server timeout");
    let address = socket.local_addr().expect("read fake server address");
    let worker = thread::spawn(move || {
        let mut request = [0u8; 2048];
        let (length, peer) = socket.recv_from(&mut request)?;
        assert_eq!(length, NTP_PACKET_LEN);
        assert_eq!(request[0], 0x23, "request must be NTPv4 client mode");

        let mut response = [0u8; NTP_PACKET_LEN];
        response[0] = 0x24; // LI=0, VN=4, server mode=4.
        response[1] = 2; // A synchronized, stratum-2 fake server.
        let server_time = current_ntp_timestamp();
        write_timestamp(&mut response, 32, server_time);
        write_timestamp(&mut response, 40, server_time);
        socket.send_to(&response, peer)?;
        Ok(())
    });
    (address, worker)
}

#[test]
fn transport_queries_a_loopback_server_and_service_assembles_measurement() {
    let (server, worker) = spawn_fake_server("127.0.0.1:0".parse().unwrap());
    let transport = NtpTransport::new(TEST_TIMEOUT);
    let sent_at = SystemTime::now();
    let response = transport
        .query_addr(server)
        .expect("transport should receive the local response");
    let received_at = SystemTime::now();
    worker
        .join()
        .expect("fake server thread must not panic")
        .unwrap();

    assert_eq!(response.server, server);
    assert!(response.round_trip_time() <= TEST_TIMEOUT);
    let header = response
        .header()
        .expect("transport returned a valid header");
    assert_eq!(header.mode, 4);
    assert_eq!(header.stratum, 2);

    let result = master_time::service::assemble_result(server, header, sent_at, received_at)
        .expect("service should assemble the local exchange");
    assert_eq!(result.server, server);
    assert_eq!(result.header, header);
    assert!(result.measurement.offset.is_finite());
    assert!(result.measurement.round_trip_delay.is_finite());
    assert!(result.measurement.round_trip_delay >= 0.0);
    assert_eq!(result.measurement.root_distance, 0.0);
    assert_eq!(
        result.timestamps.originate,
        Some(MeasurementTimestamp::new(
            master_time::service::system_time_to_ntp_timestamp(sent_at)
                .unwrap()
                .seconds,
            master_time::service::system_time_to_ntp_timestamp(sent_at)
                .unwrap()
                .fraction,
        ))
    );
}

#[test]
fn measurement_service_queries_a_loopback_server_on_ntp_port() {
    let _port_lock = NTP_PORT_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (server, worker) = spawn_fake_server("127.0.0.1:123".parse().unwrap());
    let profile = ServerProfile::new("Local fake NTP", "127.0.0.1", None).unwrap();
    let service = NtpMeasurementService::new(NtpTransport::new(TEST_TIMEOUT));

    let result = service.measure(&profile);
    worker
        .join()
        .expect("fake server thread must not panic")
        .unwrap();
    let result = result.expect("measurement service should query the local server");

    assert_eq!(result.server, server);
    assert_eq!(result.header.mode, 4);
    assert_eq!(result.header.stratum, 2);
    assert!(result.measurement.offset.is_finite());
    assert!(result.measurement.round_trip_delay >= 0.0);
}

#[test]
fn transport_rejects_a_zero_timeout_without_network_access() {
    let result = NtpTransport::new(Duration::ZERO).query_addr("127.0.0.1:9".parse().unwrap());
    assert!(matches!(result, Err(TransportError::InvalidTimeout)));
}
