use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use aes_siv::aead::{Aead, KeyInit, Payload};
use aes_siv::{Aes128SivAead, Nonce};
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

use super::{
    NTP_HEADER_LEN, NtpError, NtpSample, NtpTimestamp, client_request, parse_response,
    validate_root_distance,
};

const DEFAULT_NTS_KE_PORT: u16 = 4460;
const DEFAULT_NTP_PORT: u16 = 123;
const NTS_NEXT_PROTOCOL: &[u8] = b"ntske/1";
const NTS_EXPORTER_LABEL: &[u8] = b"EXPORTER-network-time-security";
const NTPV4_PROTOCOL_ID: u16 = 0;
const AEAD_AES_SIV_CMAC_256: u16 = 15;
const AES_SIV_KEY_LEN: usize = 32;
const AES_SIV_NONCE_LEN: usize = 16;
const UNIQUE_ID_LEN: usize = 32;
const MAX_KE_RESPONSE_LEN: usize = 65_536;
const MAX_NTP_PACKET_LEN: usize = 1_232;
const MAX_COOKIES: usize = 8;
const QUERY_ATTEMPTS_PER_ADDRESS: usize = 2;
const MIN_KE_RETRY: Duration = Duration::from_secs(10);
const MAX_KE_RETRY: Duration = Duration::from_hours(120);

const KE_END_OF_MESSAGE: u16 = 0;
const KE_NEXT_PROTOCOL: u16 = 1;
const KE_ERROR: u16 = 2;
const KE_WARNING: u16 = 3;
const KE_AEAD: u16 = 4;
const KE_NEW_COOKIE: u16 = 5;
const KE_NTP_SERVER: u16 = 6;
const KE_NTP_PORT: u16 = 7;

const EF_UNIQUE_IDENTIFIER: u16 = 0x0104;
const EF_COOKIE: u16 = 0x0204;
const EF_COOKIE_PLACEHOLDER: u16 = 0x0304;
const EF_AUTHENTICATOR: u16 = 0x0404;

/// Stateful RFC 8915 client. Each query uses a fresh NTS cookie and retains
/// replacement cookies returned by the authenticated NTP server.
#[derive(Debug)]
pub struct NtsClient {
    ke_server: String,
    timeout: Duration,
    max_root_distance: Duration,
    session: Option<NtsSession>,
    next_ke_attempt: Option<Instant>,
    ke_retry_delay: Duration,
}

impl NtsClient {
    /// Configure an NTS client. `server` names the NTS-KE service and may
    /// include a port; port 4460 is used by default. No network I/O occurs
    /// until [`Self::query`] is called.
    #[must_use]
    pub fn new(server: impl Into<String>, timeout: Duration) -> Self {
        Self {
            ke_server: server.into(),
            timeout,
            max_root_distance: super::DEFAULT_MAX_ROOT_DISTANCE,
            session: None,
            next_ke_attempt: None,
            ke_retry_delay: MIN_KE_RETRY,
        }
    }

    /// Set the maximum accepted root synchronization distance.
    #[must_use]
    pub const fn with_max_root_distance(mut self, distance: Duration) -> Self {
        self.max_root_distance = distance;
        self
    }

    /// Perform an authenticated NTP query, establishing a new TLS session
    /// when the local cookie supply is empty.
    ///
    /// # Errors
    ///
    /// Returns an error when NTS-KE, DNS, transport, authentication, or NTP
    /// response validation fails. An NTS error is never downgraded to NTP.
    pub fn query(&mut self) -> Result<NtpSample, NtpError> {
        if self
            .session
            .as_ref()
            .is_none_or(|session| session.cookies.is_empty())
        {
            if let Some(remaining) = self
                .next_ke_attempt
                .and_then(|attempt| attempt.checked_duration_since(Instant::now()))
            {
                return Err(NtpError::Nts(format!(
                    "key-establishment retry is rate-limited for {:.1}s",
                    remaining.as_secs_f64()
                )));
            }
            match establish_session(&self.ke_server, self.timeout) {
                Ok(session) => self.session = Some(session),
                Err(error) => {
                    self.schedule_ke_retry();
                    return Err(error);
                }
            }
        }

        let endpoint = self
            .session
            .as_ref()
            .map(NtsSession::endpoint)
            .ok_or_else(|| NtpError::Nts("key establishment produced no session".into()))?;
        let addresses: Vec<_> = endpoint.to_socket_addrs()?.collect();
        if addresses.is_empty() {
            self.session = None;
            return Err(NtpError::NoAddress(endpoint));
        }

        let maximum_attempts = addresses.len() * QUERY_ATTEMPTS_PER_ADDRESS;
        let mut last_error = None;
        for address in addresses.into_iter().cycle().take(maximum_attempts) {
            let result = self.query_addr(address);
            match result {
                Ok(sample) => {
                    self.next_ke_attempt = None;
                    self.ke_retry_delay = MIN_KE_RETRY;
                    return Ok(sample);
                }
                Err(error) => last_error = Some(error),
            }
            if self
                .session
                .as_ref()
                .is_none_or(|session| session.cookies.is_empty())
            {
                break;
            }
        }

        // Do not retain association state after a failed authenticated volley.
        // This also ensures a consumed cookie is never retried on a later poll.
        self.session = None;
        self.schedule_ke_retry();
        Err(last_error.unwrap_or(NtpError::NoUsableSample))
    }

    fn schedule_ke_retry(&mut self) {
        self.next_ke_attempt = Instant::now().checked_add(self.ke_retry_delay);
        self.ke_retry_delay = self.ke_retry_delay.mul_f64(1.5).min(MAX_KE_RETRY);
    }

    fn query_addr(&mut self, server: SocketAddr) -> Result<NtpSample, NtpError> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| NtpError::Nts("missing key-establishment state".into()))?;
        let cookie = session
            .cookies
            .pop()
            .ok_or_else(|| NtpError::Nts("NTS cookie supply is empty".into()))?;
        let placeholders = 7_usize.saturating_sub(session.cookies.len()).min(7);

        let bind_address = if server.is_ipv4() {
            SocketAddr::from(([0, 0, 0, 0], 0))
        } else {
            SocketAddr::from(([0_u16; 8], 0))
        };
        let socket = UdpSocket::bind(bind_address)?;
        socket.connect(server)?;
        socket.set_read_timeout(Some(self.timeout))?;
        socket.set_write_timeout(Some(self.timeout))?;

        let departed = SystemTime::now();
        let transmit = NtpTimestamp::from_system_time(departed)?;
        let (request, unique_id) =
            build_request(transmit, &cookie, placeholders, &session.c2s_key)?;
        let sent = socket.send(&request)?;
        if sent != request.len() {
            return Err(NtpError::InvalidResponse("partial UDP request"));
        }

        let mut response_bytes = [0_u8; MAX_NTP_PACKET_LEN];
        let received_len = socket.recv(&mut response_bytes)?;
        let destination = NtpTimestamp::from_system_time(SystemTime::now())?;
        let bytes = &response_bytes[..received_len];

        let replacement_cookies = authenticate_response(bytes, &unique_id, &session.s2c_key)?;
        let response = parse_response(bytes, transmit)?;
        let sample = response.into_sample(server, transmit, destination, true);
        validate_root_distance(&sample, self.max_root_distance)?;

        session.cookies.extend(replacement_cookies);
        if session.cookies.len() > MAX_COOKIES {
            session.cookies.drain(..session.cookies.len() - MAX_COOKIES);
        }
        Ok(sample)
    }
}

struct NtsSession {
    ntp_host: String,
    ntp_port: u16,
    c2s_key: [u8; AES_SIV_KEY_LEN],
    s2c_key: [u8; AES_SIV_KEY_LEN],
    cookies: Vec<Vec<u8>>,
}

impl std::fmt::Debug for NtsSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NtsSession")
            .field("ntp_host", &self.ntp_host)
            .field("ntp_port", &self.ntp_port)
            .field("cookies", &self.cookies.len())
            .finish_non_exhaustive()
    }
}

impl NtsSession {
    fn endpoint(&self) -> String {
        self.ntp_host.parse::<IpAddr>().map_or_else(
            |_| format!("{}:{}", self.ntp_host.trim_end_matches('.'), self.ntp_port),
            |address| SocketAddr::new(address, self.ntp_port).to_string(),
        )
    }
}

fn establish_session(server: &str, timeout: Duration) -> Result<NtsSession, NtpError> {
    let (tls_host, port) = split_host_and_port(server, DEFAULT_NTS_KE_PORT)?;
    let endpoint = endpoint(&tls_host, port);
    let addresses: Vec<_> = endpoint.to_socket_addrs()?.collect();
    if addresses.is_empty() {
        return Err(NtpError::NoAddress(server.to_owned()));
    }

    let mut last_error = None;
    for address in addresses {
        match establish_session_addr(&tls_host, address, timeout) {
            Ok(session) => return Ok(session),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| NtpError::NoAddress(server.to_owned())))
}

fn establish_session_addr(
    tls_host: &str,
    address: SocketAddr,
    timeout: Duration,
) -> Result<NtsSession, NtpError> {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let mut config = ClientConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
        .with_root_certificates(roots)
        .with_no_client_auth();
    config.alpn_protocols = vec![NTS_NEXT_PROTOCOL.to_vec()];

    let server_name = ServerName::try_from(tls_host.to_owned())
        .map_err(|error| NtpError::Nts(format!("invalid TLS server name: {error}")))?;
    let connection = ClientConnection::new(Arc::new(config), server_name)
        .map_err(|error| NtpError::Nts(format!("cannot create TLS client: {error}")))?;
    let socket = TcpStream::connect_timeout(&address, timeout)?;
    socket.set_read_timeout(Some(timeout))?;
    socket.set_write_timeout(Some(timeout))?;
    let peer_ip = socket.peer_addr()?.ip();
    let mut stream = StreamOwned::new(connection, socket);

    stream.write_all(&ke_request()?)?;
    // NTS-KE is a single-request protocol. RFC 8915 requires the client to
    // send close_notify after its request while continuing to read the peer's
    // response on the other half of the TLS connection.
    stream.conn.send_close_notify();
    stream.flush()?;
    if stream.conn.alpn_protocol() != Some(NTS_NEXT_PROTOCOL) {
        return Err(NtpError::Nts(
            "TLS server did not negotiate the ntske/1 ALPN protocol".into(),
        ));
    }
    let response = read_ke_response(&mut stream)?;
    let negotiated = parse_ke_response(&response, peer_ip)?;

    let algorithm = AEAD_AES_SIV_CMAC_256.to_be_bytes();
    let context_prefix = [0, 0, algorithm[0], algorithm[1]];
    let mut c2s_context = [0_u8; 5];
    c2s_context[..4].copy_from_slice(&context_prefix);
    let mut s2c_context = c2s_context;
    s2c_context[4] = 1;
    let c2s_key = stream
        .conn
        .export_keying_material(
            [0_u8; AES_SIV_KEY_LEN],
            NTS_EXPORTER_LABEL,
            Some(&c2s_context),
        )
        .map_err(|error| NtpError::Nts(format!("cannot export C2S key: {error}")))?;
    let s2c_key = stream
        .conn
        .export_keying_material(
            [0_u8; AES_SIV_KEY_LEN],
            NTS_EXPORTER_LABEL,
            Some(&s2c_context),
        )
        .map_err(|error| NtpError::Nts(format!("cannot export S2C key: {error}")))?;
    Ok(NtsSession {
        ntp_host: negotiated.ntp_host,
        ntp_port: negotiated.ntp_port,
        c2s_key,
        s2c_key,
        cookies: negotiated.cookies,
    })
}

fn ke_request() -> Result<Vec<u8>, NtpError> {
    let mut request = Vec::with_capacity(18);
    append_ke_record(
        &mut request,
        true,
        KE_NEXT_PROTOCOL,
        &NTPV4_PROTOCOL_ID.to_be_bytes(),
    )?;
    append_ke_record(
        &mut request,
        true,
        KE_AEAD,
        &AEAD_AES_SIV_CMAC_256.to_be_bytes(),
    )?;
    append_ke_record(&mut request, true, KE_END_OF_MESSAGE, &[])?;
    Ok(request)
}

fn append_ke_record(
    message: &mut Vec<u8>,
    critical: bool,
    record_type: u16,
    body: &[u8],
) -> Result<(), NtpError> {
    let encoded_type = record_type | if critical { 0x8000 } else { 0 };
    let encoded_length = u16::try_from(body.len())
        .map_err(|_| NtpError::Nts("NTS-KE record body is too large".into()))?;
    message.extend_from_slice(&encoded_type.to_be_bytes());
    message.extend_from_slice(&encoded_length.to_be_bytes());
    message.extend_from_slice(body);
    Ok(())
}

fn read_ke_response(stream: &mut impl Read) -> Result<Vec<u8>, NtpError> {
    let mut response = Vec::new();
    loop {
        let mut header = [0_u8; 4];
        stream.read_exact(&mut header)?;
        let body_len = usize::from(u16::from_be_bytes([header[2], header[3]]));
        if response.len() + header.len() + body_len > MAX_KE_RESPONSE_LEN {
            return Err(NtpError::Nts("NTS-KE response exceeds 65536 bytes".into()));
        }
        response.extend_from_slice(&header);
        let old_len = response.len();
        response.resize(old_len + body_len, 0);
        stream.read_exact(&mut response[old_len..])?;
        if u16::from_be_bytes([header[0], header[1]]) & 0x7fff == KE_END_OF_MESSAGE {
            return Ok(response);
        }
    }
}

struct KeNegotiated {
    ntp_host: String,
    ntp_port: u16,
    cookies: Vec<Vec<u8>>,
}

#[derive(Default)]
struct KeResponseBuilder {
    next_protocol: Option<Vec<u16>>,
    aead: Option<Vec<u16>>,
    ntp_host: Option<String>,
    ntp_port: Option<u16>,
    cookies: Vec<Vec<u8>>,
}

impl KeResponseBuilder {
    fn process(&mut self, record_type: u16, critical: bool, body: &[u8]) -> Result<(), NtpError> {
        match record_type {
            KE_NEXT_PROTOCOL => {
                if self.next_protocol.replace(parse_u16_list(body)?).is_some() || !critical {
                    return Err(NtpError::Nts("invalid NTS next-protocol record".into()));
                }
            }
            KE_ERROR => {
                let code = parse_single_u16(body, "NTS-KE error")?;
                return Err(NtpError::Nts(format!(
                    "NTS-KE server returned error {code}"
                )));
            }
            KE_WARNING => {
                let code = parse_single_u16(body, "NTS-KE warning")?;
                return Err(NtpError::Nts(format!(
                    "NTS-KE server returned warning {code}"
                )));
            }
            KE_AEAD => {
                if self.aead.replace(parse_u16_list(body)?).is_some() {
                    return Err(NtpError::Nts("duplicate NTS AEAD record".into()));
                }
            }
            KE_NEW_COOKIE => {
                if body.is_empty() {
                    return Err(NtpError::Nts(
                        "NTS-KE server returned an empty cookie".into(),
                    ));
                }
                if self.cookies.len() < MAX_COOKIES {
                    self.cookies.push(body.to_vec());
                }
            }
            KE_NTP_SERVER => self.process_ntp_server(body)?,
            KE_NTP_PORT => {
                if self
                    .ntp_port
                    .replace(parse_single_u16(body, "NTP port")?)
                    .is_some()
                {
                    return Err(NtpError::Nts("duplicate NTP port record".into()));
                }
            }
            _ if critical => {
                return Err(NtpError::Nts(format!(
                    "unrecognized critical NTS-KE record {record_type}"
                )));
            }
            _ => {}
        }
        Ok(())
    }

    fn process_ntp_server(&mut self, body: &[u8]) -> Result<(), NtpError> {
        if self.ntp_host.is_some() || body.is_empty() || !body.is_ascii() {
            return Err(NtpError::Nts(
                "invalid NTP server negotiation record".into(),
            ));
        }
        let host = std::str::from_utf8(body)
            .map_err(|error| NtpError::Nts(format!("invalid NTP server name: {error}")))?;
        if host
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        {
            return Err(NtpError::Nts("unsafe NTP server name".into()));
        }
        self.ntp_host = Some(host.to_owned());
        Ok(())
    }

    fn finish(self, peer_ip: IpAddr) -> Result<KeNegotiated, NtpError> {
        if self.next_protocol.as_deref() != Some(&[NTPV4_PROTOCOL_ID]) {
            return Err(NtpError::Nts("NTS-KE server did not select NTPv4".into()));
        }
        if self.aead.as_deref() != Some(&[AEAD_AES_SIV_CMAC_256]) {
            return Err(NtpError::Nts(
                "NTS-KE server did not select AEAD_AES_SIV_CMAC_256".into(),
            ));
        }
        if self.cookies.is_empty() {
            return Err(NtpError::Nts("NTS-KE server returned no cookies".into()));
        }
        let port = self.ntp_port.unwrap_or(DEFAULT_NTP_PORT);
        if port == 0 {
            return Err(NtpError::Nts("NTS-KE server selected UDP port zero".into()));
        }
        Ok(KeNegotiated {
            ntp_host: self.ntp_host.unwrap_or_else(|| peer_ip.to_string()),
            ntp_port: port,
            cookies: self.cookies,
        })
    }
}

fn parse_ke_response(bytes: &[u8], peer_ip: IpAddr) -> Result<KeNegotiated, NtpError> {
    let mut cursor = 0;
    let mut builder = KeResponseBuilder::default();
    let mut saw_end = false;

    while cursor < bytes.len() {
        let header = bytes
            .get(cursor..cursor + 4)
            .ok_or_else(|| NtpError::Nts("truncated NTS-KE record header".into()))?;
        let encoded_type = u16::from_be_bytes([header[0], header[1]]);
        let critical = encoded_type & 0x8000 != 0;
        let record_type = encoded_type & 0x7fff;
        let body_len = usize::from(u16::from_be_bytes([header[2], header[3]]));
        cursor += 4;
        let body = bytes
            .get(cursor..cursor + body_len)
            .ok_or_else(|| NtpError::Nts("truncated NTS-KE record body".into()))?;
        cursor += body_len;

        match record_type {
            KE_END_OF_MESSAGE => {
                if !critical || !body.is_empty() || cursor != bytes.len() || saw_end {
                    return Err(NtpError::Nts("invalid NTS-KE end-of-message record".into()));
                }
                saw_end = true;
            }
            _ => builder.process(record_type, critical, body)?,
        }
    }

    if !saw_end {
        return Err(NtpError::Nts(
            "NTS-KE response has no end-of-message".into(),
        ));
    }
    builder.finish(peer_ip)
}

fn parse_u16_list(body: &[u8]) -> Result<Vec<u16>, NtpError> {
    if !body.len().is_multiple_of(2) {
        return Err(NtpError::Nts("odd-length NTS-KE integer list".into()));
    }
    Ok(body
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
        .collect())
}

fn parse_single_u16(body: &[u8], name: &str) -> Result<u16, NtpError> {
    let bytes: [u8; 2] = body
        .try_into()
        .map_err(|_| NtpError::Nts(format!("invalid {name} record")))?;
    Ok(u16::from_be_bytes(bytes))
}

fn build_request(
    transmit: NtpTimestamp,
    cookie: &[u8],
    requested_placeholders: usize,
    key: &[u8; AES_SIV_KEY_LEN],
) -> Result<(Vec<u8>, [u8; UNIQUE_ID_LEN]), NtpError> {
    let mut unique_id = [0_u8; UNIQUE_ID_LEN];
    getrandom::fill(&mut unique_id)
        .map_err(|error| NtpError::Nts(format!("cannot generate unique identifier: {error}")))?;
    let mut nonce = [0_u8; AES_SIV_NONCE_LEN];
    getrandom::fill(&mut nonce)
        .map_err(|error| NtpError::Nts(format!("cannot generate NTS nonce: {error}")))?;

    let mut packet = client_request(transmit).to_vec();
    append_extension(&mut packet, EF_UNIQUE_IDENTIFIER, &unique_id)?;
    append_extension(&mut packet, EF_COOKIE, cookie)?;
    let placeholder_body_len = padded_body_len(cookie.len());
    let placeholder = vec![0_u8; placeholder_body_len];
    for _ in 0..requested_placeholders {
        let previous_len = packet.len();
        append_extension(&mut packet, EF_COOKIE_PLACEHOLDER, &placeholder)?;
        if estimated_authenticated_packet_len(packet.len()) > MAX_NTP_PACKET_LEN {
            packet.truncate(previous_len);
            break;
        }
    }

    let cipher = Aes128SivAead::new_from_slice(key)
        .map_err(|_| NtpError::Nts("invalid AES-SIV key length".into()))?;
    let nonce_array: &Nonce = nonce
        .as_slice()
        .try_into()
        .map_err(|_| NtpError::Nts("invalid nonce length while constructing NTS request".into()))?;
    let ciphertext = cipher
        .encrypt(
            nonce_array,
            Payload {
                msg: &[],
                aad: &packet,
            },
        )
        .map_err(|_| NtpError::Nts("cannot authenticate NTS request".into()))?;
    let mut authenticator = Vec::with_capacity(4 + nonce.len() + ciphertext.len());
    let nonce_len =
        u16::try_from(nonce.len()).map_err(|_| NtpError::Nts("NTS nonce is too large".into()))?;
    let ciphertext_len = u16::try_from(ciphertext.len())
        .map_err(|_| NtpError::Nts("NTS ciphertext is too large".into()))?;
    authenticator.extend_from_slice(&nonce_len.to_be_bytes());
    authenticator.extend_from_slice(&ciphertext_len.to_be_bytes());
    authenticator.extend_from_slice(&nonce);
    authenticator.extend_from_slice(&ciphertext);
    append_extension(&mut packet, EF_AUTHENTICATOR, &authenticator)?;
    if packet.len() > MAX_NTP_PACKET_LEN {
        return Err(NtpError::Nts(
            "NTS request exceeds safe UDP packet size".into(),
        ));
    }
    Ok((packet, unique_id))
}

const fn estimated_authenticated_packet_len(associated_data_len: usize) -> usize {
    associated_data_len + 4 + 4 + AES_SIV_NONCE_LEN + 16
}

fn authenticate_response(
    packet: &[u8],
    expected_unique_id: &[u8; UNIQUE_ID_LEN],
    key: &[u8; AES_SIV_KEY_LEN],
) -> Result<Vec<Vec<u8>>, NtpError> {
    if packet.len() < NTP_HEADER_LEN + 4 {
        return Err(NtpError::NtsAuthentication);
    }
    let mut cursor = NTP_HEADER_LEN;
    let mut unique_identifier_count = 0;
    let mut authenticator = None;

    while cursor + 4 <= packet.len() {
        let field_start = cursor;
        let field_type = u16::from_be_bytes([packet[cursor], packet[cursor + 1]]);
        let field_len = usize::from(u16::from_be_bytes([packet[cursor + 2], packet[cursor + 3]]));
        if field_len < 4 || field_len % 4 != 0 || cursor + field_len > packet.len() {
            return Err(NtpError::NtsAuthentication);
        }
        let body = &packet[cursor + 4..cursor + field_len];
        cursor += field_len;
        match field_type {
            EF_UNIQUE_IDENTIFIER => {
                unique_identifier_count += 1;
                if body != expected_unique_id {
                    return Err(NtpError::NtsAuthentication);
                }
            }
            EF_AUTHENTICATOR => {
                if authenticator.is_some() {
                    return Err(NtpError::NtsAuthentication);
                }
                authenticator = Some((field_start, body));
                break;
            }
            _ => {}
        }
    }
    if unique_identifier_count != 1 {
        return Err(NtpError::NtsAuthentication);
    }
    let (associated_data_len, body) = authenticator.ok_or(NtpError::NtsAuthentication)?;
    let (nonce, ciphertext) = parse_authenticator_body(body)?;
    let cipher = Aes128SivAead::new_from_slice(key).map_err(|_| NtpError::NtsAuthentication)?;
    let nonce_array: &Nonce = nonce.try_into().map_err(|_| NtpError::NtsAuthentication)?;
    let plaintext = cipher
        .decrypt(
            nonce_array,
            Payload {
                msg: ciphertext,
                aad: &packet[..associated_data_len],
            },
        )
        .map_err(|_| NtpError::NtsAuthentication)?;
    parse_encrypted_cookies(&plaintext)
}

fn parse_authenticator_body(body: &[u8]) -> Result<(&[u8], &[u8]), NtpError> {
    if body.len() < 4 {
        return Err(NtpError::NtsAuthentication);
    }
    let nonce_len = usize::from(u16::from_be_bytes([body[0], body[1]]));
    let ciphertext_len = usize::from(u16::from_be_bytes([body[2], body[3]]));
    if nonce_len != AES_SIV_NONCE_LEN || ciphertext_len < 16 {
        return Err(NtpError::NtsAuthentication);
    }
    let nonce_end = 4 + padded_body_len(nonce_len);
    let ciphertext_end = nonce_end + padded_body_len(ciphertext_len);
    if ciphertext_end > body.len() {
        return Err(NtpError::NtsAuthentication);
    }
    if body[4 + nonce_len..nonce_end]
        .iter()
        .chain(body[nonce_end + ciphertext_len..].iter())
        .any(|byte| *byte != 0)
    {
        return Err(NtpError::NtsAuthentication);
    }
    Ok((
        &body[4..4 + nonce_len],
        &body[nonce_end..nonce_end + ciphertext_len],
    ))
}

fn parse_encrypted_cookies(plaintext: &[u8]) -> Result<Vec<Vec<u8>>, NtpError> {
    let mut cursor = 0;
    let mut cookies = Vec::new();
    while cursor < plaintext.len() {
        if cursor + 4 > plaintext.len() {
            return Err(NtpError::NtsAuthentication);
        }
        let field_type = u16::from_be_bytes([plaintext[cursor], plaintext[cursor + 1]]);
        let field_len = usize::from(u16::from_be_bytes([
            plaintext[cursor + 2],
            plaintext[cursor + 3],
        ]));
        if field_len < 4 || field_len % 4 != 0 || cursor + field_len > plaintext.len() {
            return Err(NtpError::NtsAuthentication);
        }
        if field_type == EF_COOKIE {
            let cookie = &plaintext[cursor + 4..cursor + field_len];
            if cookie.is_empty() {
                return Err(NtpError::NtsAuthentication);
            }
            if cookies.len() < MAX_COOKIES {
                cookies.push(cookie.to_vec());
            }
        }
        cursor += field_len;
    }
    Ok(cookies)
}

fn append_extension(packet: &mut Vec<u8>, field_type: u16, body: &[u8]) -> Result<(), NtpError> {
    let body_len = padded_body_len(body.len());
    let field_len = body_len
        .checked_add(4)
        .ok_or_else(|| NtpError::Nts("NTP extension length overflow".into()))?;
    let encoded_len =
        u16::try_from(field_len).map_err(|_| NtpError::Nts("NTP extension is too large".into()))?;
    packet.extend_from_slice(&field_type.to_be_bytes());
    packet.extend_from_slice(&encoded_len.to_be_bytes());
    packet.extend_from_slice(body);
    packet.resize(packet.len() + body_len - body.len(), 0);
    Ok(())
}

const fn padded_body_len(length: usize) -> usize {
    length.saturating_add(3) & !3
}

fn split_host_and_port(server: &str, default_port: u16) -> Result<(String, u16), NtpError> {
    if let Ok(address) = server.parse::<SocketAddr>() {
        return Ok((address.ip().to_string(), address.port()));
    }
    if let Ok(address) = server.parse::<IpAddr>() {
        return Ok((address.to_string(), default_port));
    }
    if let Some(bracketed) = server.strip_prefix('[') {
        if let Some((host, port)) = bracketed.split_once("]:") {
            return Ok((host.to_owned(), parse_port(port)?));
        }
        if let Some(host) = bracketed.strip_suffix(']') {
            return Ok((host.to_owned(), default_port));
        }
    }
    if let Some((host, port)) = server.rsplit_once(':')
        && !host.contains(':')
    {
        return Ok((host.to_owned(), parse_port(port)?));
    }
    if server.is_empty() {
        return Err(NtpError::Nts("empty NTS-KE server name".into()));
    }
    Ok((server.to_owned(), default_port))
}

fn parse_port(value: &str) -> Result<u16, NtpError> {
    let port = value
        .parse::<u16>()
        .map_err(|_| NtpError::Nts("invalid NTS-KE port".into()))?;
    if port == 0 {
        Err(NtpError::Nts("NTS-KE port must not be zero".into()))
    } else {
        Ok(port)
    }
}

fn endpoint(host: &str, port: u16) -> String {
    host.parse::<IpAddr>().map_or_else(
        |_| format!("{host}:{port}"),
        |address| SocketAddr::new(address, port).to_string(),
    )
}

impl From<rustls::Error> for NtpError {
    fn from(error: rustls::Error) -> Self {
        Self::Nts(format!("TLS failure: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    fn ke_response() -> Result<Vec<u8>, NtpError> {
        let mut response = Vec::new();
        append_ke_record(&mut response, true, KE_NEXT_PROTOCOL, &[0, 0])?;
        append_ke_record(&mut response, false, KE_AEAD, &[0, 15])?;
        append_ke_record(&mut response, false, KE_NEW_COOKIE, &[1, 2, 3, 4])?;
        append_ke_record(&mut response, false, KE_NTP_SERVER, b"time.example")?;
        append_ke_record(&mut response, false, KE_NTP_PORT, &[0x04, 0xd2])?;
        append_ke_record(&mut response, true, KE_END_OF_MESSAGE, &[])?;
        Ok(response)
    }

    #[test]
    fn key_exchange_request_offers_required_protocol_and_aead() -> Result<(), NtpError> {
        assert_eq!(
            ke_request()?,
            [
                0x80, 0x01, 0, 2, 0, 0, 0x80, 0x04, 0, 2, 0, 15, 0x80, 0, 0, 0,
            ]
        );
        Ok(())
    }

    #[test]
    fn parses_key_exchange_response() -> Result<(), NtpError> {
        let parsed = parse_ke_response(&ke_response()?, IpAddr::from([192, 0, 2, 1]))?;
        assert_eq!(parsed.ntp_host, "time.example");
        assert_eq!(parsed.ntp_port, 1_234);
        assert_eq!(parsed.cookies, [vec![1, 2, 3, 4]]);
        Ok(())
    }

    #[test]
    fn rejects_unknown_critical_key_exchange_record() -> Result<(), NtpError> {
        let mut response = Vec::new();
        append_ke_record(&mut response, true, 42, &[])?;
        append_ke_record(&mut response, true, KE_END_OF_MESSAGE, &[])?;
        assert!(parse_ke_response(&response, IpAddr::from([127, 0, 0, 1])).is_err());
        Ok(())
    }

    #[test]
    fn request_has_unique_cookie_placeholders_and_authenticator() -> Result<(), NtpError> {
        let transmit = NtpTimestamp::from_raw(123_u64 << 32);
        let (request, unique_id) = build_request(transmit, &[7; 16], 2, &[9; 32])?;
        assert!(request.len() <= MAX_NTP_PACKET_LEN);
        assert_eq!(&request[52..84], &unique_id);
        assert_eq!(
            u16::from_be_bytes([request[48], request[49]]),
            EF_UNIQUE_IDENTIFIER
        );
        assert!(
            request
                .windows(2)
                .any(|bytes| bytes == EF_AUTHENTICATOR.to_be_bytes())
        );
        Ok(())
    }

    #[test]
    fn authenticates_response_and_extracts_encrypted_cookie() -> Result<(), NtpError> {
        let unique_id = [4_u8; UNIQUE_ID_LEN];
        let key = [8_u8; AES_SIV_KEY_LEN];
        let nonce = [3_u8; AES_SIV_NONCE_LEN];
        let mut packet = [0_u8; NTP_HEADER_LEN].to_vec();
        packet[0] = (4 << 3) | 4;
        append_extension(&mut packet, EF_UNIQUE_IDENTIFIER, &unique_id)?;

        let mut plaintext = Vec::new();
        append_extension(&mut plaintext, EF_COOKIE, &[6; 16])?;
        let cipher = Aes128SivAead::new_from_slice(&key)
            .map_err(|_| NtpError::Nts("test key failed".into()))?;
        let nonce_array: &Nonce = nonce
            .as_slice()
            .try_into()
            .map_err(|_| NtpError::Nts("test nonce failed".into()))?;
        let ciphertext = cipher
            .encrypt(
                nonce_array,
                Payload {
                    msg: &plaintext,
                    aad: &packet,
                },
            )
            .map_err(|_| NtpError::Nts("test encryption failed".into()))?;
        let mut body = Vec::new();
        body.extend_from_slice(&16_u16.to_be_bytes());
        let ciphertext_len = u16::try_from(ciphertext.len())
            .map_err(|_| NtpError::Nts("test ciphertext is too large".into()))?;
        body.extend_from_slice(&ciphertext_len.to_be_bytes());
        body.extend_from_slice(&nonce);
        body.extend_from_slice(&ciphertext);
        append_extension(&mut packet, EF_AUTHENTICATOR, &body)?;

        assert_eq!(
            authenticate_response(&packet, &unique_id, &key)?,
            [vec![6; 16]]
        );
        packet[10] ^= 1;
        assert!(matches!(
            authenticate_response(&packet, &unique_id, &key),
            Err(NtpError::NtsAuthentication)
        ));
        Ok(())
    }

    #[test]
    fn parses_named_and_ipv6_endpoints() -> Result<(), NtpError> {
        assert_eq!(
            split_host_and_port("time.example:4461", 4460)?,
            ("time.example".into(), 4461)
        );
        assert_eq!(
            split_host_and_port("[2001:db8::1]:4461", 4460)?,
            ("2001:db8::1".into(), 4461)
        );
        Ok(())
    }

    #[test]
    fn eof_before_key_exchange_end_is_an_io_error() {
        assert!(read_ke_response(&mut io::Cursor::new([0x80, 0, 0, 0])).is_ok());
        assert!(read_ke_response(&mut io::Cursor::new([0x80, 1, 0, 2, 0])).is_err());
    }

    #[test]
    #[ignore = "requires public Internet access"]
    fn live_cloudflare_interoperability() -> Result<(), NtpError> {
        let timeout = Duration::from_secs(5);
        let address = SocketAddr::from(([162, 159, 200, 123], DEFAULT_NTS_KE_PORT));
        let session = establish_session_addr("time.cloudflare.com", address, timeout)?;
        let mut client = NtsClient::new("time.cloudflare.com", timeout);
        client.session = Some(session);
        let sample = client.query()?;
        assert!(sample.authenticated);
        Ok(())
    }
}
