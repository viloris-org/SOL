//! Unix-domain transport for SCP.
//!
//! Messages are Protobuf payloads prefixed by a four-byte big-endian length.
//! Client buffer descriptors are delivered out-of-band with SCM_RIGHTS.

use crate::scp::{
    event_queue::{OutboundEvent, SessionSink},
    protocol::{ClientMessage, CompositorMessage, SessionId},
    security::{DaemonSecurityCoordinator, SecurityCoordinator, StubSecurityCoordinator},
    state::ScpState,
    unix_socket,
    wire::WireMessage,
};
use std::{
    io::{self, Read, Write},
    os::fd::AsRawFd,
    os::unix::{
        fs::{FileTypeExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub const DEFAULT_SOCKET_NAME: &str = "sol-compositor-0";
pub const MAX_FRAME_SIZE: usize = 1024 * 1024;
const SHARED_SOCKET_ENV: &str = "SOL_SCP_SHARED_SOCKET";
/// Explicit test-only escape hatch for subprocess integration fixtures.
/// Production service definitions never set this variable.
const INSECURE_STUB_SECURITY_ENV: &str = "SOL_SCP_INSECURE_STUB_SECURITY";

/// How long a client thread waits before rechecking its socket and event queue.
///
/// Both wake sources are edge-triggered, so this bound is only a backstop
/// against a missed wakeup, not part of normal operation.
const POLL_TIMEOUT: Duration = Duration::from_millis(250);

/// How long a write to a client may stall before the client is given up on.
///
/// Generous enough that a busy but healthy client is never dropped, short enough
/// that a wedged one does not hold a thread and its queued descriptors forever.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Bound the thread-per-client transport so a connect flood cannot exhaust the
/// compositor's descriptors or address space.
pub const MAX_CLIENT_CONNECTIONS: usize = 256;

/// Unauthenticated peers receive no events and must complete their handshake
/// promptly instead of occupying a worker forever.
#[cfg(not(test))]
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(test)]
const HANDSHAKE_TIMEOUT: Duration = Duration::from_millis(100);

/// Running SCP listener. Dropping it stops the accept loop and removes the
/// filesystem socket. Active client threads observe shutdown through the same
/// bounded poll loop and tear down their sessions before returning.
pub struct ScpServer {
    socket_path: PathBuf,
    state: Arc<Mutex<ScpState>>,
    shutdown: Arc<AtomicBool>,
    accept_thread: Option<JoinHandle<()>>,
    client_threads: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

impl ScpServer {
    pub fn bind_from_env() -> io::Result<Self> {
        let mode = if std::env::var_os(SHARED_SOCKET_ENV).as_deref() == Some("1".as_ref()) {
            0o666
        } else {
            0o600
        };
        let security: Arc<dyn SecurityCoordinator> =
            if std::env::var_os(INSECURE_STUB_SECURITY_ENV).as_deref() == Some("1".as_ref()) {
                tracing::warn!(
                    "using insecure in-process SCP security stub; this is only valid in tests"
                );
                Arc::new(StubSecurityCoordinator::default())
            } else {
                Arc::new(DaemonSecurityCoordinator::from_env())
            };
        Self::bind_with_state_and_mode(
            resolve_socket_path()?,
            Arc::new(Mutex::new(ScpState::with_security(security))),
            mode,
        )
    }

    pub fn bind(socket_path: PathBuf) -> io::Result<Self> {
        Self::bind_with_state(socket_path, Arc::new(Mutex::new(ScpState::new())))
    }

    pub fn bind_with_state(socket_path: PathBuf, state: Arc<Mutex<ScpState>>) -> io::Result<Self> {
        Self::bind_with_state_and_mode(socket_path, state, 0o600)
    }

    fn bind_with_state_and_mode(
        socket_path: PathBuf,
        state: Arc<Mutex<ScpState>>,
        mode: u32,
    ) -> io::Result<Self> {
        let listener = bind_listener(&socket_path, mode)?;
        listener.set_nonblocking(true)?;

        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = Arc::clone(&shutdown);
        let thread_state = Arc::clone(&state);
        let active_connections = Arc::new(AtomicUsize::new(0));
        let client_threads = Arc::new(Mutex::new(Vec::new()));
        let thread_clients = Arc::clone(&client_threads);
        let accept_thread = thread::Builder::new()
            .name("scp-listener".to_string())
            .spawn(move || {
                accept_loop(
                    listener,
                    thread_state,
                    thread_shutdown,
                    active_connections,
                    thread_clients,
                )
            })?;

        tracing::info!(socket = %socket_path.display(), "SCP listener ready");
        Ok(Self {
            socket_path,
            state,
            shutdown,
            accept_thread: Some(accept_thread),
            client_threads,
        })
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Shared state consumed by the native renderer/input loop.
    pub fn state(&self) -> Arc<Mutex<ScpState>> {
        Arc::clone(&self.state)
    }
}

/// Bind the SCP socket, and publish it only once it has its final mode.
///
/// `bind` creates the socket with the process umask, so setting the mode
/// afterwards leaves a window in which the path already exists with the wrong
/// access. Binding under a temporary name, setting the mode there, and renaming
/// into place closes that window; `rename` publishes it atomically.
///
/// Production uses a shared system socket so the authenticated user's UID can
/// attach to the compositor that owns the seat. That mode does not grant SCP
/// authority by itself: transport peer credentials are still checked against
/// the UID explicitly admitted by the greeter.
fn bind_listener(socket_path: &Path, mode: u32) -> io::Result<UnixListener> {
    let parent = socket_path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "SCP socket path has no parent directory",
        )
    })?;
    let file_name = socket_path.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "SCP socket path has no name")
    })?;

    claim_socket_path(socket_path)?;

    // Same directory as the target, so the rename below stays within one
    // filesystem and is therefore atomic.
    let staging = parent.join(format!(
        ".{}.{}.staging",
        file_name.to_string_lossy(),
        std::process::id()
    ));
    let _ = std::fs::remove_file(&staging);

    let listener = UnixListener::bind(&staging)?;
    if let Err(error) = std::fs::set_permissions(&staging, std::fs::Permissions::from_mode(mode))
        .and_then(|()| std::fs::rename(&staging, socket_path))
    {
        drop(listener);
        let _ = std::fs::remove_file(&staging);
        return Err(error);
    }

    Ok(listener)
}

/// Make sure nothing else owns the published path, clearing a stale socket.
fn claim_socket_path(socket_path: &Path) -> io::Result<()> {
    let metadata = match std::fs::symlink_metadata(socket_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };

    if !metadata.file_type().is_socket() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "refusing to replace a non-socket SCP path",
        ));
    }

    // A socket that still accepts connections belongs to a live compositor.
    match UnixStream::connect(socket_path) {
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            "another compositor is listening on this SCP socket",
        )),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
            ) =>
        {
            std::fs::remove_file(socket_path)
        }
        Err(error) => Err(error),
    }
}

impl Drop for ScpServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(thread) = self.accept_thread.take() {
            let _ = thread.join();
        }
        let workers: Vec<_> = lock_workers(&self.client_threads).drain(..).collect();
        for worker in workers {
            let _ = worker.join();
        }
        match std::fs::remove_file(&self.socket_path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                tracing::warn!(?error, path = %self.socket_path.display(), "failed to remove SCP socket")
            }
        }
    }
}

pub fn resolve_socket_path() -> io::Result<PathBuf> {
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "XDG_RUNTIME_DIR is not set"))?;
    if !runtime_dir.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "XDG_RUNTIME_DIR must be absolute",
        ));
    }
    let configured = std::env::var_os("SOL_SCP_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_NAME));
    if configured.is_absolute() {
        Ok(configured)
    } else {
        Ok(runtime_dir.join(configured))
    }
}

fn accept_loop(
    listener: UnixListener,
    state: Arc<Mutex<ScpState>>,
    shutdown: Arc<AtomicBool>,
    active_connections: Arc<AtomicUsize>,
    client_threads: Arc<Mutex<Vec<JoinHandle<()>>>>,
) {
    while !shutdown.load(Ordering::Acquire) {
        reap_finished_workers(&client_threads);
        match listener.accept() {
            Ok((stream, _)) => {
                if active_connections
                    .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                        (active < MAX_CLIENT_CONNECTIONS).then_some(active + 1)
                    })
                    .is_err()
                {
                    let mut stream = stream;
                    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
                    let _ = write_protocol_error(
                        &mut stream,
                        "server-busy",
                        "SCP connection limit reached; retry later".to_string(),
                        true,
                    );
                    continue;
                }
                let state = Arc::clone(&state);
                let client_shutdown = Arc::clone(&shutdown);
                let connection_count = Arc::clone(&active_connections);
                match thread::Builder::new()
                    .name("scp-client".to_string())
                    .spawn(move || {
                        let _connection = ActiveConnection(connection_count);
                        if let Err(error) = serve_client(stream, &state, &client_shutdown) {
                            tracing::debug!(?error, "SCP client disconnected with error");
                        }
                    }) {
                    Ok(worker) => lock_workers(&client_threads).push(worker),
                    Err(error) => {
                        active_connections.fetch_sub(1, Ordering::AcqRel);
                        tracing::error!(?error, "failed to spawn SCP client worker");
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                tracing::error!(?error, "SCP accept failed");
                thread::sleep(Duration::from_millis(50));
            }
        }
    }
    reap_finished_workers(&client_threads);
}

fn lock_workers(
    workers: &Arc<Mutex<Vec<JoinHandle<()>>>>,
) -> std::sync::MutexGuard<'_, Vec<JoinHandle<()>>> {
    workers.lock().unwrap_or_else(|poisoned| {
        tracing::warn!("SCP worker registry mutex was poisoned; recovering");
        poisoned.into_inner()
    })
}

fn reap_finished_workers(workers: &Arc<Mutex<Vec<JoinHandle<()>>>>) {
    let finished = {
        let mut workers = lock_workers(workers);
        let mut finished = Vec::new();
        let mut active = Vec::with_capacity(workers.len());
        for worker in workers.drain(..) {
            if worker.is_finished() {
                finished.push(worker);
            } else {
                active.push(worker);
            }
        }
        *workers = active;
        finished
    };
    for worker in finished {
        let _ = worker.join();
    }
}

struct ActiveConnection(Arc<AtomicUsize>);

impl Drop for ActiveConnection {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

fn serve_client(
    mut stream: UnixStream,
    state: &Arc<Mutex<ScpState>>,
    shutdown: &AtomicBool,
) -> io::Result<()> {
    // A client that stops reading fills its socket buffer, and a blocking write
    // then parks this thread indefinitely — including before it can reach the
    // event-queue overflow check that exists to disconnect exactly this client.
    // The deadline turns "wedged forever" into "disconnected", which is what the
    // overflow path already decided is the right answer.
    stream.set_write_timeout(Some(WRITE_TIMEOUT))?;

    let mut session_id = None;
    let mut bytes = ReceiveBuffer::new();
    let mut pending_fds = Vec::new();
    let sink = SessionSink::new()?;

    let result = serve_client_inner(
        &mut stream,
        state,
        &sink,
        &mut session_id,
        &mut bytes,
        &mut pending_fds,
        shutdown,
    );

    close_all(&mut pending_fds);
    // Closing the sink first stops other threads from queueing events — and
    // closes the descriptors of any that are still pending — before the session
    // is torn down.
    sink.close();
    if let Some(session_id) = session_id {
        lock_state(state).disconnect(session_id);
    }
    result
}

/// Acquire the shared compositor state, recovering from a poisoned lock.
///
/// A poisoned lock means some client thread panicked while holding it. Treating
/// that as fatal would let one malformed request disconnect every other client
/// and leave the state permanently unreachable, so the panic is contained to the
/// thread that caused it and everyone else keeps running.
fn lock_state(state: &Arc<Mutex<ScpState>>) -> std::sync::MutexGuard<'_, ScpState> {
    state.lock().unwrap_or_else(|poisoned| {
        tracing::error!("SCP state mutex was poisoned by a panicking client thread; recovering");
        poisoned.into_inner()
    })
}

fn serve_client_inner(
    stream: &mut UnixStream,
    state: &Arc<Mutex<ScpState>>,
    sink: &Arc<SessionSink>,
    session_id: &mut Option<SessionId>,
    bytes: &mut ReceiveBuffer,
    pending_fds: &mut Vec<i32>,
    shutdown: &AtomicBool,
) -> io::Result<()> {
    let (peer_pid, peer_uid, _) = unix_socket::get_peer_credentials(stream.as_raw_fd())?;
    let accepted_at = Instant::now();

    while !shutdown.load(Ordering::Acquire) {
        // Two things can need this thread: the client sending a request, or
        // another thread queueing an event for it. Waiting on both is what lets
        // input and frame callbacks reach a client that is not talking.
        let readiness =
            unix_socket::poll_readable(stream.as_raw_fd(), sink.wake_fd(), POLL_TIMEOUT)?;

        if session_id.is_none() && accepted_at.elapsed() >= HANDSHAKE_TIMEOUT {
            write_protocol_error(
                stream,
                "handshake-timeout",
                "Client did not authenticate before the handshake deadline".to_string(),
                true,
            )?;
            break;
        }

        if !drain_sink(stream, sink)? {
            break;
        }

        if readiness.primary && !receive_chunk(stream, bytes, pending_fds)? {
            break;
        }
        // A hangup with no data left means the peer is gone for good.
        if readiness.hangup && !readiness.primary {
            break;
        }

        // Descriptors are only consumed once a whole frame has arrived, so a
        // client that dribbles descriptors without ever completing one would
        // otherwise accumulate them here until the compositor runs out.
        if pending_fds.len() > unix_socket::MAX_FDS_PER_MESSAGE {
            let count = pending_fds.len();
            close_all(pending_fds);
            tracing::warn!(
                count,
                "SCP client sent unattached descriptors; disconnecting"
            );
            write_protocol_error(
                stream,
                "too-many-descriptors",
                format!("received {count} descriptors with no frame to attach them to"),
                true,
            )?;
            return Ok(());
        }

        while let Some(payload) = take_frame(bytes)? {
            let message = match ClientMessage::decode_wire(&payload) {
                Ok(message) => message,
                Err(error) => {
                    close_all(pending_fds);
                    write_frame(
                        stream,
                        &CompositorMessage::ProtocolError {
                            code: "invalid-protobuf".to_string(),
                            message: error.to_string(),
                            fatal: true,
                        },
                    )?;
                    return Ok(());
                }
            };

            if let Some(pid) = connect_pid(&message)
                && pid != peer_pid
            {
                close_all(pending_fds);
                write_frame(
                    stream,
                    &CompositorMessage::Rejected {
                        reason: "claimed PID does not match SO_PEERCRED".to_string(),
                    },
                )?;
                return Ok(());
            }

            if is_connect(&message) && !lock_state(state).peer_uid_is_admitted(peer_uid) {
                close_all(pending_fds);
                write_frame(
                    stream,
                    &CompositorMessage::Rejected {
                        reason: format!(
                            "UID {peer_uid} is not authorized for the active desktop session"
                        ),
                    },
                )?;
                return Ok(());
            }

            let received_fds = match descriptor_count(&message) {
                Some(expected) if pending_fds.len() == expected => std::mem::take(pending_fds),
                Some(expected) => {
                    let count = pending_fds.len();
                    close_all(pending_fds);
                    write_protocol_error(
                        stream,
                        "invalid-fd-count",
                        format!("this request requires {expected} descriptor(s), received {count}"),
                        false,
                    )?;
                    continue;
                }
                None if pending_fds.is_empty() => Vec::new(),
                None => {
                    close_all(pending_fds);
                    write_protocol_error(
                        stream,
                        "unexpected-fd",
                        "descriptors are only accepted with buffer import requests".to_string(),
                        false,
                    )?;
                    continue;
                }
            };

            let responses = {
                let mut guard = lock_state(state);
                let responses =
                    guard.handle_transport_message(*session_id, message, received_fds, peer_uid);

                // Register this connection's outbound queue while still holding
                // the lock that created the session. Releasing it first would
                // leave a window where another thread could route an event to a
                // session that has no sink yet, and silently drop it.
                if let Ok(responses) = &responses {
                    for response in responses {
                        if let CompositorMessage::Connected {
                            session_id: connected,
                            ..
                        } = response
                        {
                            guard.register_session_sink(*connected, Arc::clone(sink));
                        }
                    }
                }
                responses
            };

            match responses {
                Ok(responses) => {
                    for response in responses {
                        if let CompositorMessage::Connected {
                            session_id: connected,
                            ..
                        } = &response
                        {
                            *session_id = Some(*connected);
                        }
                        let rejected = matches!(response, CompositorMessage::Rejected { .. });
                        write_frame(stream, &response)?;
                        if rejected {
                            return Ok(());
                        }
                    }
                }
                Err(message) => {
                    write_protocol_error(stream, "invalid-request", message, false)?;
                }
            }

            // Handling a request can queue events for this same client, so flush
            // before blocking again.
            if !drain_sink(stream, sink)? {
                return Ok(());
            }
        }
    }

    Ok(())
}

/// Write every pending event for this connection.
///
/// Returns `false` when the client has fallen too far behind and must be
/// disconnected: an unresponsive client cannot be allowed to make the
/// compositor buffer without bound.
/// Whether a request carries exactly one descriptor over SCM_RIGHTS.
///
/// Both of these hand the compositor shared memory to read from. Every other
/// request is refused a descriptor outright, so a client cannot smuggle one in
/// alongside a message that has nowhere to put it.
fn descriptor_count(message: &ClientMessage) -> Option<usize> {
    match message {
        ClientMessage::AttachBuffer { .. } | ClientMessage::CreateShmPool { .. } => Some(1),
        ClientMessage::CreateDmabufBuffer { planes, .. } => planes
            .iter()
            .map(|plane| usize::try_from(plane.fd_index).ok()?.checked_add(1))
            .try_fold(0, |highest, next| next.map(|next| highest.max(next))),
        _ => None,
    }
}

const fn connect_pid(message: &ClientMessage) -> Option<u32> {
    match message {
        ClientMessage::Connect { pid, .. } | ClientMessage::ConnectVersioned { pid, .. } => {
            Some(*pid)
        }
        _ => None,
    }
}

const fn is_connect(message: &ClientMessage) -> bool {
    matches!(
        message,
        ClientMessage::Connect { .. } | ClientMessage::ConnectVersioned { .. }
    )
}

fn drain_sink(stream: &mut UnixStream, sink: &Arc<SessionSink>) -> io::Result<bool> {
    if sink.is_overflowed() {
        tracing::warn!("SCP client is not draining its event queue; disconnecting");
        let _ = write_protocol_error(
            stream,
            "event-queue-overflow",
            "client fell too far behind on compositor events".to_string(),
            true,
        );
        return Ok(false);
    }

    for event in sink.drain() {
        write_event(stream, &event)?;
    }
    Ok(true)
}

/// Write one event, passing any attached descriptor via SCM_RIGHTS.
fn write_event(stream: &mut UnixStream, event: &OutboundEvent) -> io::Result<()> {
    match &event.fd {
        Some(fd) => write_frame_with_fd(stream, &event.message, fd.as_raw_fd()),
        None => write_frame(stream, &event.message),
    }
}

fn receive_chunk(
    stream: &UnixStream,
    bytes: &mut ReceiveBuffer,
    pending_fds: &mut Vec<i32>,
) -> io::Result<bool> {
    let mut buffer = [0_u8; 64 * 1024];
    let (received, fds) = unix_socket::recvmsg_with_fds(
        stream.as_raw_fd(),
        &mut buffer,
        unix_socket::MAX_FDS_PER_MESSAGE,
    )?;

    if received == 0 {
        return Ok(false);
    }

    pending_fds.extend(fds);
    bytes.extend(&buffer[..received]);
    Ok(true)
}

/// Bytes received but not yet parsed into frames.
///
/// A plain `Vec` with a `drain(..n)` per frame moves the whole remaining buffer
/// on every message, which is quadratic exactly when a client is busiest. A read
/// cursor makes taking a frame O(frame), and the buffer is compacted only when
/// the consumed prefix is worth reclaiming.
#[derive(Debug, Default)]
struct ReceiveBuffer {
    bytes: Vec<u8>,
    start: usize,
}

/// Consumed prefix tolerated before the buffer is compacted.
const COMPACT_THRESHOLD: usize = 64 * 1024;

impl ReceiveBuffer {
    fn new() -> Self {
        Self::default()
    }

    fn extend(&mut self, chunk: &[u8]) {
        self.bytes.extend_from_slice(chunk);
    }

    fn pending(&self) -> &[u8] {
        &self.bytes[self.start..]
    }

    fn consume(&mut self, count: usize) {
        self.start += count;
        if self.start >= self.bytes.len() {
            self.bytes.clear();
            self.start = 0;
        } else if self.start >= COMPACT_THRESHOLD {
            self.bytes.drain(..self.start);
            self.start = 0;
        }
    }
}

fn take_frame(bytes: &mut ReceiveBuffer) -> io::Result<Option<Vec<u8>>> {
    let pending = bytes.pending();
    if pending.len() < 4 {
        return Ok(None);
    }
    let frame_len = u32::from_be_bytes([pending[0], pending[1], pending[2], pending[3]]) as usize;
    if frame_len == 0 || frame_len > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid SCP frame size {frame_len}"),
        ));
    }
    if pending.len() < frame_len + 4 {
        return Ok(None);
    }
    let payload = pending[4..frame_len + 4].to_vec();
    bytes.consume(frame_len + 4);
    Ok(Some(payload))
}

fn write_protocol_error(
    stream: &mut UnixStream,
    code: &str,
    message: String,
    fatal: bool,
) -> io::Result<()> {
    write_frame(
        stream,
        &CompositorMessage::ProtocolError {
            code: code.to_string(),
            message,
            fatal,
        },
    )
}

pub fn write_frame<T: WireMessage>(stream: &mut UnixStream, value: &T) -> io::Result<()> {
    let frame = encode_frame(value)?;
    stream.write_all(&frame)
}

/// Write a frame with a descriptor attached via SCM_RIGHTS.
///
/// The descriptor rides on the first byte of the frame, so the whole frame must
/// go out in a single `sendmsg`. A short write would leave the receiver holding a
/// descriptor it cannot yet associate with a message, so it is an error rather
/// than something to retry.
///
/// Public because it is the client half of [`ClientMessage::AttachBuffer`], the
/// one request whose descriptor the compositor accepts: without it a client can
/// name a buffer but never hand one over.
pub fn write_frame_with_fd<T: WireMessage>(
    stream: &mut UnixStream,
    value: &T,
    fd: std::os::fd::RawFd,
) -> io::Result<()> {
    write_frame_with_fds(stream, value, &[fd])
}

/// Write one frame with an ordered DMA-BUF plane descriptor array.
///
/// Plane fd_index values in the message address this exact SCM_RIGHTS array.
pub fn write_frame_with_fds<T: WireMessage>(
    stream: &mut UnixStream,
    value: &T,
    fds: &[std::os::fd::RawFd],
) -> io::Result<()> {
    let frame = encode_frame(value)?;
    let sent = unix_socket::sendmsg_with_fds(stream.as_raw_fd(), &frame, fds)?;
    if sent != frame.len() {
        return Err(io::Error::new(
            io::ErrorKind::WriteZero,
            format!(
                "SCP descriptor frame was truncated: sent {sent} of {} bytes",
                frame.len()
            ),
        ));
    }
    Ok(())
}

fn encode_frame<T: WireMessage>(value: &T) -> io::Result<Vec<u8>> {
    let payload = value.encode_wire()?;
    if payload.is_empty() || payload.len() > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "SCP frame is empty or too large",
        ));
    }
    let length = u32::try_from(payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "SCP frame is too large"))?;

    let mut frame = Vec::with_capacity(payload.len() + 4);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

pub fn read_frame<T: WireMessage>(stream: &mut UnixStream) -> io::Result<T> {
    let mut length = [0_u8; 4];
    stream.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid SCP frame size {length}"),
        ));
    }
    let mut payload = vec![0_u8; length];
    stream.read_exact(&mut payload)?;
    T::decode_wire(&payload)
}

fn close_all(fds: &mut Vec<i32>) {
    for fd in fds.drain(..) {
        unix_socket::close_fd(fd);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn fixture_path(label: &str) -> PathBuf {
        static NEXT: AtomicU32 = AtomicU32::new(1);
        std::env::temp_dir().join(format!(
            "sol-scp-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn recovers_an_unowned_stale_socket() {
        let path = fixture_path("stale");
        let stale = UnixListener::bind(&path).expect("create stale socket fixture");
        drop(stale);

        let server = ScpServer::bind(path.clone()).expect("replace stale socket");
        assert_eq!(server.socket_path(), path);
        drop(server);
        assert!(!path.exists());
    }

    #[test]
    fn the_socket_is_never_reachable_before_it_is_private() {
        let path = fixture_path("mode");
        let server = ScpServer::bind(path.clone()).expect("bind");

        let mode = std::fs::metadata(&path)
            .expect("stat socket")
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "the published socket must be owner-only from the moment it exists"
        );

        drop(server);
    }

    #[test]
    fn refuses_to_take_a_socket_another_compositor_is_serving() {
        let path = fixture_path("live");
        let first = ScpServer::bind(path.clone()).expect("first compositor binds");

        let error = match ScpServer::bind(path.clone()) {
            Ok(second) => {
                drop(second);
                panic!("a live socket must not be stolen")
            }
            Err(error) => error,
        };
        assert_eq!(error.kind(), io::ErrorKind::AddrInUse);

        drop(first);
    }

    #[test]
    fn refuses_to_replace_a_regular_file() {
        let path = fixture_path("regular");
        std::fs::write(&path, b"keep").expect("create regular file fixture");

        let error = match ScpServer::bind(path.clone()) {
            Ok(server) => {
                drop(server);
                panic!("regular file must not be replaced")
            }
            Err(error) => error,
        };
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&path).expect("read fixture"), b"keep");
        std::fs::remove_file(path).expect("remove regular file fixture");
    }

    #[test]
    fn unauthenticated_connection_is_closed_after_the_handshake_deadline() {
        let path = fixture_path("handshake-timeout");
        let server = ScpServer::bind(path.clone()).expect("bind");
        let mut client = UnixStream::connect(&path).expect("connect idle client");
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set timeout");

        let response: CompositorMessage = read_frame(&mut client).expect("timeout response");
        assert!(matches!(
            response,
            CompositorMessage::ProtocolError {
                ref code,
                fatal: true,
                ..
            } if code == "handshake-timeout"
        ));
        drop(server);
    }

    #[test]
    fn server_shutdown_joins_workers_and_cleans_authenticated_sessions() {
        let path = fixture_path("clean-shutdown");
        let server = ScpServer::bind(path.clone()).expect("bind");
        let state = server.state();
        let mut client = UnixStream::connect(&path).expect("connect client");
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set timeout");
        let app_id = std::fs::read_to_string(format!("/proc/{}/comm", std::process::id()))
            .expect("read process identity")
            .trim()
            .to_string();
        write_frame(
            &mut client,
            &ClientMessage::Connect {
                app_id,
                pid: std::process::id(),
            },
        )
        .expect("send handshake");
        let response: CompositorMessage = read_frame(&mut client).expect("connected response");
        assert!(matches!(response, CompositorMessage::Connected { .. }));
        assert_eq!(lock_state(&state).session_count(), 1);

        drop(server);
        assert_eq!(
            lock_state(&state).session_count(),
            0,
            "server drop must wait for worker-owned session cleanup"
        );
    }

    #[test]
    fn dmabuf_descriptor_count_uses_the_highest_plane_index() {
        let message = ClientMessage::CreateDmabufBuffer {
            buffer_id: 1,
            width: 64,
            height: 64,
            format: crate::scp::protocol::DmabufFormat::Nv12,
            modifier: crate::scp::protocol::DRM_FORMAT_MOD_LINEAR,
            planes: vec![
                crate::scp::protocol::DmabufPlane {
                    fd_index: 0,
                    offset: 0,
                    stride: 64,
                },
                crate::scp::protocol::DmabufPlane {
                    fd_index: 1,
                    offset: 4096,
                    stride: 64,
                },
            ],
            fds: Vec::new(),
        };
        assert_eq!(descriptor_count(&message), Some(2));
    }
}
