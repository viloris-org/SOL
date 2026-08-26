//! Small, dependency-light `NTPv4` client used by the SOL time service.
//!
//! The implementation intentionally supports only unicast client/server mode.
//! It validates response provenance and timestamps, calculates the RFC 5905
//! offset and delay, and offers conservative sample selection and clock-step
//! policy. It does not implement an NTP server, symmetric modes, or NTS.

use std::error::Error;
use std::fmt;
use std::io;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const NTP_HEADER_LEN: usize = 48;
const NTP_UNIX_EPOCH_OFFSET: i128 = 2_208_988_800;
const FRACTION_SCALE: f64 = 4_294_967_296.0;
const DEFAULT_NTP_PORT: u16 = 123;

/// Default upper bound for total synchronization distance.
pub const DEFAULT_MAX_ROOT_DISTANCE: Duration = Duration::from_secs(16);

/// An error produced while resolving, querying, validating, or applying NTP.
#[derive(Debug)]
pub enum NtpError {
    /// Name resolution returned no usable address.
    NoAddress(String),
    /// A socket operation failed.
    Io(io::Error),
    /// The peer returned a malformed or unsuitable response.
    InvalidResponse(&'static str),
    /// The peer sent an NTP Kiss-o'-Death packet.
    KissOfDeath(String),
    /// No valid response remained after querying the configured sources.
    NoUsableSample,
    /// The measured correction exceeds the configured panic threshold.
    PanicThreshold {
        /// Absolute measured correction in seconds.
        offset_seconds: f64,
        /// Configured maximum accepted correction in seconds.
        limit_seconds: f64,
    },
    /// Reading or setting the platform clock failed.
    Clock(String),
}

impl fmt::Display for NtpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAddress(server) => write!(formatter, "no address found for {server}"),
            Self::Io(error) => write!(formatter, "NTP transport failed: {error}"),
            Self::InvalidResponse(reason) => write!(formatter, "invalid NTP response: {reason}"),
            Self::KissOfDeath(code) => write!(formatter, "NTP server rejected the client: {code}"),
            Self::NoUsableSample => formatter.write_str("no usable NTP sample"),
            Self::PanicThreshold {
                offset_seconds,
                limit_seconds,
            } => write!(
                formatter,
                "NTP offset {offset_seconds:.3}s exceeds panic threshold {limit_seconds:.3}s"
            ),
            Self::Clock(reason) => write!(formatter, "cannot update system clock: {reason}"),
        }
    }
}

impl Error for NtpError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for NtpError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// A 64-bit NTP timestamp (unsigned seconds and binary fraction).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NtpTimestamp(u64);

impl NtpTimestamp {
    /// Encode a wall-clock instant using the NTP epoch and current era.
    ///
    /// # Errors
    ///
    /// Returns an error if the instant cannot be represented by the integer
    /// conversion used by this implementation.
    pub fn from_system_time(time: SystemTime) -> Result<Self, NtpError> {
        let unix_nanos = system_time_to_unix_nanos(time);
        let ntp_nanos = unix_nanos
            .checked_add(NTP_UNIX_EPOCH_OFFSET * 1_000_000_000)
            .ok_or_else(|| NtpError::Clock("timestamp overflow".into()))?;
        let seconds = ntp_nanos.div_euclid(1_000_000_000);
        let nanos = ntp_nanos.rem_euclid(1_000_000_000);
        let seconds_low = u32::try_from(seconds.rem_euclid(1_i128 << 32))
            .map_err(|_| NtpError::Clock("NTP seconds overflow".into()))?;
        let fraction = u32::try_from((nanos << 32) / 1_000_000_000)
            .map_err(|_| NtpError::Clock("NTP fraction overflow".into()))?;
        Ok(Self(u64::from(seconds_low) << 32 | u64::from(fraction)))
    }

    /// Decode a timestamp to the NTP era nearest to `pivot`.
    ///
    /// This makes timestamps continue to work across the 2036 era rollover.
    /// The result is unambiguous while the pivot is within half an era.
    ///
    /// # Errors
    ///
    /// Returns an error when the decoded instant falls outside `SystemTime`.
    pub fn to_system_time_near(self, pivot: SystemTime) -> Result<SystemTime, NtpError> {
        let pivot_unix_nanos = system_time_to_unix_nanos(pivot);
        let pivot_ntp_seconds = pivot_unix_nanos.div_euclid(1_000_000_000) + NTP_UNIX_EPOCH_OFFSET;
        let era = pivot_ntp_seconds.div_euclid(1_i128 << 32);
        let seconds_low = i128::from((self.0 >> 32) as u32);
        let candidates = [era - 1, era, era + 1].map(|candidate_era| {
            let seconds = candidate_era * (1_i128 << 32) + seconds_low;
            let distance = (seconds - pivot_ntp_seconds).abs();
            (distance, seconds)
        });
        let (_, ntp_seconds) = candidates
            .into_iter()
            .min_by_key(|(distance, _)| *distance)
            .ok_or_else(|| NtpError::Clock("cannot select NTP era".into()))?;
        let fraction = i128::from(
            u32::try_from(self.0 & u64::from(u32::MAX))
                .map_err(|_| NtpError::Clock("NTP fraction overflow".into()))?,
        );
        let nanos = fraction * 1_000_000_000 / (1_i128 << 32);
        unix_nanos_to_system_time((ntp_seconds - NTP_UNIX_EPOCH_OFFSET) * 1_000_000_000 + nanos)
    }

    /// Return `self - earlier` in seconds using NTP's wrapping arithmetic.
    #[must_use]
    pub fn seconds_since(self, earlier: Self) -> f64 {
        let [s0, s1, s2, s3, f0, f1, f2, f3] = self.0.wrapping_sub(earlier.0).to_be_bytes();
        let seconds = i32::from_be_bytes([s0, s1, s2, s3]);
        let fraction = u32::from_be_bytes([f0, f1, f2, f3]);
        f64::from(seconds) + f64::from(fraction) / FRACTION_SCALE
    }

    /// Return the on-wire fixed-point value.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }

    const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
}

/// A validated timing measurement from one NTP server.
#[derive(Debug, Clone)]
pub struct NtpSample {
    /// Address that supplied the response.
    pub server: SocketAddr,
    /// Remote clock stratum (1 through 15).
    pub stratum: u8,
    /// Server time minus local time, in seconds.
    pub offset_seconds: f64,
    /// Measured network round-trip delay, in seconds.
    pub delay_seconds: f64,
    /// Estimated total distance to the server's reference clock, in seconds.
    pub root_distance_seconds: f64,
    /// Raw four-octet reference identifier.
    pub reference_id: [u8; 4],
    /// Leap indicator supplied by the server.
    pub leap: u8,
}

/// Network client configuration.
#[derive(Debug, Clone)]
pub struct NtpClient {
    timeout: Duration,
    max_root_distance: Duration,
}

impl Default for NtpClient {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(2),
            max_root_distance: DEFAULT_MAX_ROOT_DISTANCE,
        }
    }
}

impl NtpClient {
    /// Construct a client with the given per-address timeout.
    #[must_use]
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            ..Self::default()
        }
    }

    /// Set the maximum accepted root synchronization distance.
    #[must_use]
    pub const fn with_max_root_distance(mut self, distance: Duration) -> Self {
        self.max_root_distance = distance;
        self
    }

    /// Resolve and query a host. Port 123 is used when no port is supplied.
    ///
    /// # Errors
    ///
    /// Returns the last address-specific failure, or [`NtpError::NoAddress`]
    /// when resolution produced no addresses.
    pub fn query(&self, server: &str) -> Result<NtpSample, NtpError> {
        let endpoint = endpoint_with_default_port(server);
        let addresses: Vec<_> = endpoint.to_socket_addrs()?.collect();
        if addresses.is_empty() {
            return Err(NtpError::NoAddress(server.to_owned()));
        }

        let mut last_error = None;
        for address in addresses {
            match self.query_addr(address) {
                Ok(sample) => return Ok(sample),
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.unwrap_or_else(|| NtpError::NoAddress(server.to_owned())))
    }

    /// Query one resolved NTP server address.
    ///
    /// # Errors
    ///
    /// Returns an error for transport failures, invalid protocol fields,
    /// provenance mismatches, or excessive synchronization distance.
    pub fn query_addr(&self, server: SocketAddr) -> Result<NtpSample, NtpError> {
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
        let request = client_request(transmit);
        let sent = socket.send(&request)?;
        if sent != request.len() {
            return Err(NtpError::InvalidResponse("partial UDP request"));
        }

        let mut response_bytes = [0_u8; 512];
        let received_len = socket.recv(&mut response_bytes)?;
        let arrived = SystemTime::now();
        let destination = NtpTimestamp::from_system_time(arrived)?;
        let response = parse_response(&response_bytes[..received_len], transmit)?;
        let sample = response.into_sample(server, transmit, destination);
        if sample.root_distance_seconds > self.max_root_distance.as_secs_f64() {
            return Err(NtpError::InvalidResponse(
                "root synchronization distance is too large",
            ));
        }
        Ok(sample)
    }
}

/// Query all configured sources and retain every valid response.
#[must_use]
pub fn query_sources(client: &NtpClient, servers: &[String]) -> Vec<NtpSample> {
    servers
        .iter()
        .filter_map(|server| client.query(server).ok())
        .collect()
}

/// Select the valid sample nearest the population median.
///
/// With three or more sources, samples outside four median absolute
/// deviations (with a 50 ms floor) are rejected. The survivor nearest the
/// median wins, with root distance and stratum used as tie-breakers.
#[must_use]
pub fn select_sample(samples: &[NtpSample]) -> Option<&NtpSample> {
    if samples.is_empty() {
        return None;
    }
    let mut offsets: Vec<f64> = samples.iter().map(|sample| sample.offset_seconds).collect();
    offsets.sort_by(f64::total_cmp);
    let median = median_of_sorted(&offsets);

    let tolerance = if samples.len() >= 3 {
        let mut deviations: Vec<f64> = offsets
            .iter()
            .map(|offset| (offset - median).abs())
            .collect();
        deviations.sort_by(f64::total_cmp);
        (median_of_sorted(&deviations) * 4.0).max(0.050)
    } else {
        f64::INFINITY
    };

    samples
        .iter()
        .filter(|sample| (sample.offset_seconds - median).abs() <= tolerance)
        .min_by(|left, right| {
            let left_key = (left.offset_seconds - median).abs();
            let right_key = (right.offset_seconds - median).abs();
            left_key
                .total_cmp(&right_key)
                .then_with(|| {
                    left.root_distance_seconds
                        .total_cmp(&right.root_distance_seconds)
                })
                .then_with(|| left.stratum.cmp(&right.stratum))
        })
}

/// Abstraction around a settable wall clock.
pub trait Clock {
    /// Read the current wall-clock instant.
    ///
    /// # Errors
    ///
    /// Returns an error when the platform clock cannot be read.
    fn now(&self) -> Result<SystemTime, NtpError>;

    /// Step the wall clock to an absolute instant.
    ///
    /// # Errors
    ///
    /// Returns an error when the caller lacks permission or the platform
    /// rejects the update.
    fn set(&self, time: SystemTime) -> Result<(), NtpError>;
}

/// Linux realtime clock implementation.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Result<SystemTime, NtpError> {
        Ok(SystemTime::now())
    }

    fn set(&self, time: SystemTime) -> Result<(), NtpError> {
        use nix::sys::time::TimeSpec;
        use nix::time::ClockId;

        let duration = time.duration_since(UNIX_EPOCH).map_err(|_| {
            NtpError::Clock("cannot set Linux realtime clock before Unix epoch".into())
        })?;
        let seconds = i64::try_from(duration.as_secs())
            .map_err(|_| NtpError::Clock("realtime seconds overflow".into()))?;
        ClockId::CLOCK_REALTIME
            .set_time(TimeSpec::new(seconds, i64::from(duration.subsec_nanos())))
            .map_err(|error| NtpError::Clock(error.to_string()))
    }
}

/// Policy controlling whether an NTP correction may step the clock.
#[derive(Debug, Clone, Copy)]
pub struct StepPolicy {
    /// Corrections smaller than this value are treated as already synchronized.
    pub minimum_correction: Duration,
    /// Corrections larger than this value are rejected rather than stepped.
    pub panic_threshold: Duration,
}

impl Default for StepPolicy {
    fn default() -> Self {
        Self {
            minimum_correction: Duration::from_millis(1),
            panic_threshold: Duration::from_secs(1_000),
        }
    }
}

/// Result of applying a timing sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adjustment {
    /// The correction was below the configured minimum.
    AlreadySynchronized,
    /// The wall clock was stepped.
    Stepped,
}

/// Apply a selected sample to a clock under a bounded step policy.
///
/// # Errors
///
/// Returns an error when the correction exceeds the panic threshold or when
/// the clock cannot be read or set.
pub fn apply_sample<C: Clock>(
    clock: &C,
    sample: &NtpSample,
    policy: StepPolicy,
) -> Result<Adjustment, NtpError> {
    let absolute_offset = sample.offset_seconds.abs();
    if absolute_offset > policy.panic_threshold.as_secs_f64() {
        return Err(NtpError::PanicThreshold {
            offset_seconds: absolute_offset,
            limit_seconds: policy.panic_threshold.as_secs_f64(),
        });
    }
    if absolute_offset < policy.minimum_correction.as_secs_f64() {
        return Ok(Adjustment::AlreadySynchronized);
    }

    let now = clock.now()?;
    let corrected = add_signed_seconds(now, sample.offset_seconds)?;
    clock.set(corrected)?;
    Ok(Adjustment::Stepped)
}

#[derive(Debug)]
struct ServerResponse {
    leap: u8,
    stratum: u8,
    root_delay_seconds: f64,
    root_dispersion_seconds: f64,
    reference_id: [u8; 4],
    receive: NtpTimestamp,
    transmit: NtpTimestamp,
}

impl ServerResponse {
    fn into_sample(
        self,
        server: SocketAddr,
        origin: NtpTimestamp,
        destination: NtpTimestamp,
    ) -> NtpSample {
        let outbound = self.receive.seconds_since(origin);
        let inbound = self.transmit.seconds_since(destination);
        let offset_seconds = outbound.midpoint(inbound);
        let measured_delay =
            destination.seconds_since(origin) - self.transmit.seconds_since(self.receive);
        let delay_seconds = measured_delay.max(0.0);
        let root_distance_seconds = self.root_delay_seconds.max(0.0) / 2.0
            + self.root_dispersion_seconds
            + delay_seconds / 2.0;
        NtpSample {
            server,
            stratum: self.stratum,
            offset_seconds,
            delay_seconds,
            root_distance_seconds,
            reference_id: self.reference_id,
            leap: self.leap,
        }
    }
}

fn client_request(transmit: NtpTimestamp) -> [u8; NTP_HEADER_LEN] {
    let mut packet = [0_u8; NTP_HEADER_LEN];
    packet[0] = (4 << 3) | 3;
    packet[40..48].copy_from_slice(&transmit.raw().to_be_bytes());
    packet
}

fn parse_response(bytes: &[u8], expected_origin: NtpTimestamp) -> Result<ServerResponse, NtpError> {
    if bytes.len() < NTP_HEADER_LEN || !bytes.len().is_multiple_of(4) {
        return Err(NtpError::InvalidResponse(
            "packet must contain an aligned 48-byte header",
        ));
    }
    let leap = bytes[0] >> 6;
    let version = (bytes[0] >> 3) & 0x07;
    let mode = bytes[0] & 0x07;
    if !(3..=4).contains(&version) {
        return Err(NtpError::InvalidResponse("unsupported NTP version"));
    }
    if mode != 4 {
        return Err(NtpError::InvalidResponse("packet is not a server response"));
    }
    if leap == 3 {
        return Err(NtpError::InvalidResponse("server is unsynchronized"));
    }

    let stratum = bytes[1];
    let reference_id: [u8; 4] = bytes[12..16]
        .try_into()
        .map_err(|_| NtpError::InvalidResponse("missing reference identifier"))?;
    if stratum == 0 {
        let code = String::from_utf8_lossy(&reference_id)
            .trim_end_matches('\0')
            .to_owned();
        return Err(NtpError::KissOfDeath(if code.is_empty() {
            "UNKNOWN".to_owned()
        } else {
            code
        }));
    }
    if stratum > 15 {
        return Err(NtpError::InvalidResponse("invalid stratum"));
    }

    let origin = read_timestamp(bytes, 24)?;
    if origin != expected_origin {
        return Err(NtpError::InvalidResponse(
            "origin timestamp does not match request",
        ));
    }
    let receive = read_timestamp(bytes, 32)?;
    let transmit = read_timestamp(bytes, 40)?;
    if receive.raw() == 0 || transmit.raw() == 0 {
        return Err(NtpError::InvalidResponse("server timestamp is zero"));
    }

    Ok(ServerResponse {
        leap,
        stratum,
        root_delay_seconds: f64::from(i32::from_be_bytes(
            bytes[4..8]
                .try_into()
                .map_err(|_| NtpError::InvalidResponse("missing root delay"))?,
        )) / 65_536.0,
        root_dispersion_seconds: f64::from(u32::from_be_bytes(
            bytes[8..12]
                .try_into()
                .map_err(|_| NtpError::InvalidResponse("missing root dispersion"))?,
        )) / 65_536.0,
        reference_id,
        receive,
        transmit,
    })
}

fn read_timestamp(bytes: &[u8], offset: usize) -> Result<NtpTimestamp, NtpError> {
    let raw = u64::from_be_bytes(
        bytes
            .get(offset..offset + 8)
            .ok_or(NtpError::InvalidResponse("missing timestamp"))?
            .try_into()
            .map_err(|_| NtpError::InvalidResponse("invalid timestamp"))?,
    );
    Ok(NtpTimestamp::from_raw(raw))
}

fn endpoint_with_default_port(server: &str) -> String {
    if let Ok(address) = server.parse::<SocketAddr>() {
        return address.to_string();
    }
    if let Ok(address) = server.parse::<IpAddr>() {
        return SocketAddr::new(address, DEFAULT_NTP_PORT).to_string();
    }
    if server
        .rsplit_once(':')
        .is_some_and(|(_, port)| port.parse::<u16>().is_ok())
    {
        server.to_owned()
    } else {
        format!("{server}:{DEFAULT_NTP_PORT}")
    }
}

const fn median_of_sorted(values: &[f64]) -> f64 {
    let midpoint = values.len() / 2;
    if values.len().is_multiple_of(2) {
        values[midpoint - 1].midpoint(values[midpoint])
    } else {
        values[midpoint]
    }
}

fn add_signed_seconds(time: SystemTime, seconds: f64) -> Result<SystemTime, NtpError> {
    if !seconds.is_finite() {
        return Err(NtpError::Clock("non-finite NTP correction".into()));
    }
    let magnitude = Duration::from_secs_f64(seconds.abs());
    if seconds.is_sign_negative() {
        time.checked_sub(magnitude)
    } else {
        time.checked_add(magnitude)
    }
    .ok_or_else(|| NtpError::Clock("corrected time is outside SystemTime".into()))
}

fn system_time_to_unix_nanos(time: SystemTime) -> i128 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => {
            i128::from(duration.as_secs()) * 1_000_000_000 + i128::from(duration.subsec_nanos())
        }
        Err(error) => {
            let duration = error.duration();
            -(i128::from(duration.as_secs()) * 1_000_000_000 + i128::from(duration.subsec_nanos()))
        }
    }
}

fn unix_nanos_to_system_time(unix_nanos: i128) -> Result<SystemTime, NtpError> {
    let magnitude = unix_nanos.unsigned_abs();
    let seconds = u64::try_from(magnitude / 1_000_000_000)
        .map_err(|_| NtpError::Clock("decoded seconds overflow".into()))?;
    let nanos = u32::try_from(magnitude % 1_000_000_000)
        .map_err(|_| NtpError::Clock("decoded nanoseconds overflow".into()))?;
    let duration = Duration::new(seconds, nanos);
    if unix_nanos.is_negative() {
        UNIX_EPOCH.checked_sub(duration)
    } else {
        UNIX_EPOCH.checked_add(duration)
    }
    .ok_or_else(|| NtpError::Clock("decoded NTP timestamp is outside SystemTime".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn timestamp(seconds: u32, nanos: u32) -> NtpTimestamp {
        let fraction = u64::from(nanos) * (1_u64 << 32) / 1_000_000_000;
        NtpTimestamp::from_raw((u64::from(seconds) << 32) | fraction)
    }

    fn response(origin: NtpTimestamp, receive: NtpTimestamp, transmit: NtpTimestamp) -> [u8; 48] {
        let mut packet = [0_u8; 48];
        packet[0] = (4 << 3) | 4;
        packet[1] = 2;
        packet[8..12].copy_from_slice(&655_u32.to_be_bytes());
        packet[12..16].copy_from_slice(&[192, 0, 2, 1]);
        packet[24..32].copy_from_slice(&origin.raw().to_be_bytes());
        packet[32..40].copy_from_slice(&receive.raw().to_be_bytes());
        packet[40..48].copy_from_slice(&transmit.raw().to_be_bytes());
        packet
    }

    #[test]
    fn unix_epoch_has_expected_ntp_value() -> Result<(), NtpError> {
        let encoded = NtpTimestamp::from_system_time(UNIX_EPOCH)?;
        assert_eq!(encoded.raw(), 2_208_988_800_u64 << 32);
        Ok(())
    }

    #[test]
    fn timestamp_round_trips_across_2036_era() -> Result<(), NtpError> {
        let instant = UNIX_EPOCH + Duration::from_secs(2_200_000_000);
        let encoded = NtpTimestamp::from_system_time(instant)?;
        let decoded = encoded.to_system_time_near(instant)?;
        assert!(decoded.duration_since(instant).unwrap_or_default() < Duration::from_nanos(1));
        Ok(())
    }

    #[test]
    fn client_packet_uses_v4_client_mode_and_transmit_timestamp() {
        let transmit = timestamp(123, 500_000_000);
        let packet = client_request(transmit);
        assert_eq!(packet[0], 0x23);
        assert_eq!(&packet[40..48], &transmit.raw().to_be_bytes());
    }

    #[test]
    fn timestamp_difference_preserves_negative_fraction() {
        let earlier = timestamp(1_001, 0);
        let later = timestamp(1_000, 500_000_000);
        assert!((later.seconds_since(earlier) + 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn computes_rfc_offset_and_delay() -> Result<(), NtpError> {
        let t1 = timestamp(1_000, 0);
        let t2 = timestamp(1_000, 125_000_000);
        let t3 = timestamp(1_000, 150_000_000);
        let t4 = timestamp(1_000, 75_000_000);
        let parsed = parse_response(&response(t1, t2, t3), t1)?;
        let sample = parsed.into_sample(SocketAddr::from(([127, 0, 0, 1], 123)), t1, t4);
        assert!((sample.offset_seconds - 0.100).abs() < 0.000_001);
        assert!((sample.delay_seconds - 0.050).abs() < 0.000_001);
        Ok(())
    }

    #[test]
    fn rejects_replayed_response_with_wrong_origin() {
        let t1 = timestamp(1_000, 0);
        let packet = response(
            timestamp(999, 0),
            timestamp(1_000, 100_000_000),
            timestamp(1_000, 200_000_000),
        );
        assert!(matches!(
            parse_response(&packet, t1),
            Err(NtpError::InvalidResponse(
                "origin timestamp does not match request"
            ))
        ));
    }

    #[test]
    fn reports_kiss_of_death() {
        let t1 = timestamp(1_000, 0);
        let mut packet = response(
            t1,
            timestamp(1_000, 100_000_000),
            timestamp(1_000, 200_000_000),
        );
        packet[1] = 0;
        packet[12..16].copy_from_slice(b"RATE");
        assert!(matches!(
            parse_response(&packet, t1),
            Err(NtpError::KissOfDeath(code)) if code == "RATE"
        ));
    }

    #[test]
    fn selection_ignores_large_outlier() -> Result<(), NtpError> {
        let sample = |offset_seconds| NtpSample {
            server: SocketAddr::from(([127, 0, 0, 1], 123)),
            stratum: 2,
            offset_seconds,
            delay_seconds: 0.010,
            root_distance_seconds: 0.020,
            reference_id: [0; 4],
            leap: 0,
        };
        let samples = [sample(0.010), sample(0.011), sample(60.0)];
        let selected = select_sample(&samples).ok_or(NtpError::NoUsableSample)?;
        assert!((selected.offset_seconds - 0.011).abs() < f64::EPSILON);
        Ok(())
    }

    #[derive(Debug)]
    struct FakeClock {
        now: SystemTime,
        set_to: Mutex<Option<SystemTime>>,
    }

    impl Clock for FakeClock {
        fn now(&self) -> Result<SystemTime, NtpError> {
            Ok(self.now)
        }

        fn set(&self, time: SystemTime) -> Result<(), NtpError> {
            *self
                .set_to
                .lock()
                .map_err(|error| NtpError::Clock(error.to_string()))? = Some(time);
            Ok(())
        }
    }

    #[test]
    fn applies_bounded_clock_step() -> Result<(), NtpError> {
        let clock = FakeClock {
            now: UNIX_EPOCH + Duration::from_secs(10),
            set_to: Mutex::new(None),
        };
        let sample = NtpSample {
            server: SocketAddr::from(([127, 0, 0, 1], 123)),
            stratum: 2,
            offset_seconds: -0.25,
            delay_seconds: 0.01,
            root_distance_seconds: 0.02,
            reference_id: [0; 4],
            leap: 0,
        };
        assert_eq!(
            apply_sample(&clock, &sample, StepPolicy::default())?,
            Adjustment::Stepped
        );
        let set_to = clock
            .set_to
            .lock()
            .map_err(|error| NtpError::Clock(error.to_string()))?
            .ok_or_else(|| NtpError::Clock("fake clock was not set".into()))?;
        assert_eq!(
            clock
                .now
                .duration_since(set_to)
                .map_err(|error| NtpError::Clock(error.to_string()))?,
            Duration::from_millis(250)
        );
        Ok(())
    }
}
