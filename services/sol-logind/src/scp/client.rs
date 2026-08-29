//! SCP transport for the login screen.
//!
//! Owns the Unix socket and pumps [`LockDriver`]. Everything about *what* to
//! send lives in the driver; this module only deals with bytes, descriptors, and
//! waiting.

use std::{
    io,
    os::{fd::AsRawFd, unix::net::UnixStream},
    time::Duration,
};

use sol_compositor::scp::{
    protocol::{ClientMessage, CompositorMessage},
    resolve_socket_path,
    transport::{MAX_FRAME_SIZE, write_frame, write_frame_with_fd},
    unix_socket,
};

use super::{
    buffer::FrameBuffer,
    lock::{LockDriver, LockError, LockEvent, LockPhase},
};

/// How long to wait for the compositor to finish the lock handshake.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Read chunk size. Frames are small; this only bounds one `recvmsg`.
const READ_CHUNK: usize = 64 * 1024;

/// A connected, locked login screen.
pub struct ScpClient {
    stream: UnixStream,
    driver: LockDriver,
    /// Bytes received but not yet forming a complete frame.
    pending: Vec<u8>,
    /// Events produced while pumping that the caller has not drained yet.
    events: Vec<LockEvent>,
}

impl ScpClient {
    /// Connect to the compositor and engage the session lock.
    ///
    /// Returns once the compositor reports `SessionLocked` — every output is
    /// covered and keyboard focus is on the login screen.
    pub fn connect() -> Result<Self, Error> {
        let socket_path = resolve_socket_path().map_err(Error::Socket)?;
        let stream = UnixStream::connect(&socket_path).map_err(|error| Error::Connect {
            path: socket_path.display().to_string(),
            error,
        })?;
        // A blocking write is fine: frames are small and the compositor drains
        // continuously. Reads are driven by `poll` instead of a socket timeout,
        // so a slow compositor cannot desynchronize a half-read frame.
        stream
            .set_write_timeout(Some(HANDSHAKE_TIMEOUT))
            .map_err(Error::Socket)?;

        let mut client = Self {
            stream,
            driver: LockDriver::new(),
            pending: Vec::new(),
            events: Vec::new(),
        };

        let app_id = process_app_id().map_err(Error::Socket)?;
        let connect = client.driver.start(app_id, std::process::id());
        client.send(&connect)?;
        client.run_handshake()?;
        Ok(client)
    }

    /// Size of the lock surface the login UI should render at.
    pub const fn size(&self) -> (i32, i32) {
        self.driver.size()
    }

    pub const fn is_locked(&self) -> bool {
        self.driver.is_locked()
    }

    pub const fn phase(&self) -> &LockPhase {
        self.driver.phase()
    }

    /// Wait up to `timeout` for compositor messages and return what they mean.
    ///
    /// A `Vec` that comes back empty simply means nothing happened in that
    /// window; disconnection is reported as [`Error::Disconnected`].
    pub fn poll(&mut self, timeout: Duration) -> Result<Vec<LockEvent>, Error> {
        // Frames already buffered from a previous read take priority: they may
        // be all the caller needs, and blocking on `poll` first would delay them
        // by a whole timeout.
        self.drain_pending_frames()?;
        if self.events.is_empty() {
            self.receive(timeout)?;
            self.drain_pending_frames()?;
        }
        Ok(std::mem::take(&mut self.events))
    }

    /// Hand the compositor a rendered frame.
    pub fn present(&mut self, buffer: &FrameBuffer) -> Result<(), Error> {
        let presentation = self
            .driver
            .present(
                buffer.as_raw_fd(),
                buffer.width(),
                buffer.height(),
                buffer.stride(),
            )
            .map_err(Error::Lock)?;

        // SCP accepts a descriptor with `AttachBuffer` and nothing else, so this
        // one frame has to go out via sendmsg while the rest are plain writes.
        write_frame_with_fd(
            &mut self.stream,
            &presentation.attach,
            presentation.buffer_fd,
        )
        .map_err(Error::Io)?;
        for message in &presentation.rest {
            self.send(message)?;
        }

        tracing::debug!(
            width = buffer.width(),
            height = buffer.height(),
            stride = buffer.stride(),
            "presented a frame to the lock surface"
        );
        Ok(())
    }

    /// Admit the authenticated user's processes before starting the desktop.
    pub fn authorize_session_user(&mut self, uid: u32) -> Result<(), Error> {
        let message = self
            .driver
            .authorize_session_user(uid)
            .map_err(Error::Lock)?;
        self.send(&message)?;
        self.wait_for_session_user(uid, true)
    }

    /// Remove a completed user's compositor admission before returning to the
    /// login screen.
    pub fn revoke_session_user(&mut self, uid: u32) -> Result<(), Error> {
        let message = self.driver.revoke_session_user(uid).map_err(Error::Lock)?;
        self.send(&message)?;
        self.wait_for_session_user(uid, false)
    }

    fn wait_for_session_user(&mut self, uid: u32, authorizing: bool) -> Result<(), Error> {
        let deadline = std::time::Instant::now() + HANDSHAKE_TIMEOUT;
        let mut deferred = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                self.events.extend(deferred);
                return Err(Error::SessionUserTimeout { uid, authorizing });
            }
            for event in self.poll(remaining)? {
                let matched = matches!(
                    event,
                    LockEvent::SessionUserAuthorized { uid: event_uid }
                        if authorizing && event_uid == uid
                ) || matches!(
                    event,
                    LockEvent::SessionUserRevoked { uid: event_uid }
                        if !authorizing && event_uid == uid
                );
                if matched {
                    self.events.extend(deferred);
                    return Ok(());
                }
                deferred.push(event);
            }
        }
    }

    /// Release the lock so the authenticated user's session can take the screen.
    pub fn unlock(&mut self) -> Result<(), Error> {
        for message in self.driver.unlock().map_err(Error::Lock)? {
            self.send(&message)?;
        }
        Ok(())
    }

    /// Lock the screen again after a user session ends.
    pub fn relock(&mut self) -> Result<(), Error> {
        let message = self.driver.relock().map_err(Error::Lock)?;
        self.send(&message)?;
        self.run_handshake()
    }

    /// Pump until the lock is engaged and confirmed.
    fn run_handshake(&mut self) -> Result<(), Error> {
        let deadline = std::time::Instant::now() + HANDSHAKE_TIMEOUT;
        while !self.driver.is_locked() {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(Error::HandshakeTimeout(self.driver.phase().clone()));
            }
            self.poll(remaining)?;
            if let LockPhase::Finished(reason) = self.driver.phase() {
                return Err(Error::Lock(LockError::Rejected(reason.clone())));
            }
        }
        tracing::info!(
            width = self.driver.size().0,
            height = self.driver.size().1,
            "session locked; login screen owns the screen and keyboard"
        );
        Ok(())
    }

    fn send(&mut self, message: &ClientMessage) -> Result<(), Error> {
        write_frame(&mut self.stream, message).map_err(Error::Io)
    }

    /// Read one chunk from the socket into `pending`.
    fn receive(&mut self, timeout: Duration) -> Result<(), Error> {
        let fd = self.stream.as_raw_fd();
        // `poll_readable` waits on two descriptors; the compositor uses the
        // second to wake its own threads. The greeter has no such wake source,
        // so the socket stands in for it and only `primary`/`hangup` are read.
        let readiness = unix_socket::poll_readable(fd, fd, timeout).map_err(Error::Io)?;
        if !readiness.primary {
            if readiness.hangup {
                return Err(Error::Disconnected);
            }
            return Ok(());
        }

        let mut chunk = [0_u8; READ_CHUNK];
        let (received, descriptors) =
            unix_socket::recvmsg_with_fds(fd, &mut chunk, unix_socket::MAX_FDS_PER_MESSAGE)
                .map_err(Error::Io)?;

        // The compositor attaches a descriptor to the keymap it sends with
        // keyboard focus, and to clipboard transfers. The greeter wants none of
        // them — it decodes keys with its own table and holds no clipboard
        // capability — so they are closed here rather than left to accumulate.
        for descriptor in descriptors {
            tracing::debug!(
                descriptor,
                "closing an unused descriptor from the compositor"
            );
            unix_socket::close_fd(descriptor);
        }

        if received == 0 {
            return Err(Error::Disconnected);
        }
        self.pending.extend_from_slice(&chunk[..received]);
        Ok(())
    }

    /// Feed every complete buffered frame through the driver.
    fn drain_pending_frames(&mut self) -> Result<(), Error> {
        while let Some(payload) = take_frame(&mut self.pending)? {
            let message: CompositorMessage = serde_json::from_slice(&payload)
                .map_err(|error| Error::Decode(error.to_string()))?;
            let step = self.driver.handle(message).map_err(Error::Lock)?;
            for message in &step.outbound {
                write_frame(&mut self.stream, message).map_err(Error::Io)?;
            }
            self.events.extend(step.events);
        }
        Ok(())
    }
}

/// Split one length-prefixed frame off the front of `bytes`, if a whole one is
/// there. Mirrors the compositor's own framing in `scp::transport`.
fn take_frame(bytes: &mut Vec<u8>) -> Result<Option<Vec<u8>>, Error> {
    if bytes.len() < 4 {
        return Ok(None);
    }
    let length = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    if length == 0 || length > MAX_FRAME_SIZE {
        return Err(Error::Decode(format!("invalid SCP frame size {length}")));
    }
    if bytes.len() < length + 4 {
        return Ok(None);
    }
    let payload = bytes[4..length + 4].to_vec();
    bytes.drain(..length + 4);
    Ok(Some(payload))
}

/// The identity the compositor will independently derive for this process.
///
/// It must be sent verbatim: `handle_connect` compares the claim against what it
/// reads from `/proc/<pid>/comm` and rejects a mismatch.
fn process_app_id() -> io::Result<String> {
    Ok(
        std::fs::read_to_string(format!("/proc/{}/comm", std::process::id()))?
            .trim()
            .to_string(),
    )
}

#[derive(Debug)]
pub enum Error {
    /// The socket path could not be resolved, or the socket configured.
    Socket(io::Error),
    Connect {
        path: String,
        error: io::Error,
    },
    Io(io::Error),
    Decode(String),
    Lock(LockError),
    HandshakeTimeout(LockPhase),
    SessionUserTimeout {
        uid: u32,
        authorizing: bool,
    },
    Disconnected,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Socket(error) => write!(f, "could not resolve the SCP socket: {error}"),
            Self::Connect { path, error } => {
                write!(
                    f,
                    "could not connect to the compositor at {path}: {error}. \
                     Start sol-compositor before sol-logind."
                )
            }
            Self::Io(error) => write!(f, "SCP transport error: {error}"),
            Self::Decode(message) => write!(f, "malformed SCP frame: {message}"),
            Self::Lock(error) => write!(f, "{error}"),
            Self::HandshakeTimeout(phase) => {
                write!(f, "the session lock did not engage; stalled at {phase:?}")
            }
            Self::SessionUserTimeout { uid, authorizing } => write!(
                f,
                "timed out while {} compositor access for UID {uid}",
                if *authorizing {
                    "authorizing"
                } else {
                    "revoking"
                }
            ),
            Self::Disconnected => write!(f, "the compositor closed the connection"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Socket(error) | Self::Connect { error, .. } | Self::Io(error) => Some(error),
            Self::Lock(error) => Some(error),
            _ => None,
        }
    }
}

/// Whether a raw descriptor is still open, used to keep buffer handling honest
/// in tests.
#[cfg(test)]
fn is_open(fd: std::os::fd::RawFd) -> bool {
    // SAFETY: F_GETFD only reads flags and never blocks.
    unsafe { libc::fcntl(fd, libc::F_GETFD) != -1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(payload: &[u8]) -> Vec<u8> {
        let mut bytes = (payload.len() as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(payload);
        bytes
    }

    #[test]
    fn take_frame_waits_for_a_complete_frame() {
        let mut bytes = vec![0, 0, 0, 8, b'a'];
        assert!(
            take_frame(&mut bytes)
                .expect("partial frame is not an error")
                .is_none()
        );
        assert_eq!(bytes.len(), 5, "an incomplete frame is left buffered");
    }

    #[test]
    fn take_frame_splits_back_to_back_frames() {
        let mut bytes = frame(b"first");
        bytes.extend(frame(b"second"));

        assert_eq!(
            take_frame(&mut bytes).expect("first frame").as_deref(),
            Some(&b"first"[..])
        );
        assert_eq!(
            take_frame(&mut bytes).expect("second frame").as_deref(),
            Some(&b"second"[..])
        );
        assert!(take_frame(&mut bytes).expect("no more frames").is_none());
        assert!(bytes.is_empty());
    }

    #[test]
    fn take_frame_rejects_impossible_lengths() {
        for length in [0_u32, (MAX_FRAME_SIZE + 1) as u32] {
            let mut bytes = length.to_be_bytes().to_vec();
            bytes.extend_from_slice(b"payload");
            assert!(
                take_frame(&mut bytes).is_err(),
                "frame length {length} must be rejected"
            );
        }
    }

    #[test]
    fn a_missing_compositor_names_the_socket_and_the_fix() {
        let error = Error::Connect {
            path: "/run/user/1000/sol-compositor-0".to_string(),
            error: io::Error::from(io::ErrorKind::NotFound),
        };
        let message = error.to_string();
        assert!(
            message.contains("/run/user/1000/sol-compositor-0"),
            "{message}"
        );
        assert!(message.contains("Start sol-compositor"), "{message}");
    }

    #[test]
    fn a_frame_buffer_descriptor_stays_open_for_reuse() {
        let buffer = FrameBuffer::new(8, 8).expect("allocate buffer");
        let fd = buffer.as_raw_fd();
        assert!(is_open(fd));
        drop(buffer);
        assert!(!is_open(fd), "dropping the buffer must close its memfd");
    }
}
