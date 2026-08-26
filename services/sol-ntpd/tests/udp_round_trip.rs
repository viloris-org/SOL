use std::error::Error;
use std::net::UdpSocket;
use std::thread;
use std::time::{Duration, SystemTime};

use sol_ntpd::{NtpClient, NtpTimestamp};

#[test]
fn queries_a_local_udp_server() -> Result<(), Box<dyn Error>> {
    let server = UdpSocket::bind("127.0.0.1:0")?;
    server.set_read_timeout(Some(Duration::from_secs(2)))?;
    let address = server.local_addr()?;
    let responder = thread::spawn(move || -> Result<(), String> {
        let mut request = [0_u8; 48];
        let (_, client) = server
            .recv_from(&mut request)
            .map_err(|error| error.to_string())?;
        let origin: [u8; 8] = request[40..48]
            .try_into()
            .map_err(|_| "request has no transmit timestamp".to_owned())?;
        let receive =
            NtpTimestamp::from_system_time(SystemTime::now()).map_err(|error| error.to_string())?;
        let transmit =
            NtpTimestamp::from_system_time(SystemTime::now()).map_err(|error| error.to_string())?;

        let mut response = [0_u8; 48];
        response[0] = (4 << 3) | 4;
        response[1] = 2;
        response[8..12].copy_from_slice(&655_u32.to_be_bytes());
        response[12..16].copy_from_slice(&[192, 0, 2, 10]);
        response[24..32].copy_from_slice(&origin);
        response[32..40].copy_from_slice(&receive.raw().to_be_bytes());
        response[40..48].copy_from_slice(&transmit.raw().to_be_bytes());
        server
            .send_to(&response, client)
            .map_err(|error| error.to_string())?;
        Ok(())
    });

    let sample = NtpClient::new(Duration::from_secs(2)).query_addr(address)?;
    responder
        .join()
        .map_err(|_| "local NTP responder panicked")??;
    assert_eq!(sample.server, address);
    assert_eq!(sample.stratum, 2);
    assert!(sample.offset_seconds.abs() < 0.100);
    assert!(sample.delay_seconds < 0.100);
    Ok(())
}
