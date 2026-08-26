//! Unix-domain transport for SCP.
//!
//! Messages are JSON payloads prefixed by a four-byte big-endian length.
//! Client buffer descriptors are delivered out-of-band with SCM_RIGHTS.

use crate::scp::{
    protocol::{ClientMessage, CompositorMessage, SessionId},
    state::ScpState,
    unix_socket,
};
use serde::{Serialize, de::DeserializeOwned};
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
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

pub const DEFAULT_SOCKET_NAME: &str = "sol-compositor-0";
pub const MAX_FRAME_SIZE: usize = 1024 * 1024;

/// Running SCP listener. Dropping it stops the accept loop and removes the
/// filesystem socket. Active client threads finish when their peers disconnect.
pub struct ScpServer {
    socket_path: PathBuf,
    state: Arc<Mutex<ScpState>>,
    shutdown: Arc<AtomicBool>,
    accept_thread: Option<JoinHandle<()>>,
}

impl ScpServer {
    pub fn bind_from_env() -> io::Result<Self> {
        Self::bind(resolve_socket_path()?)
    }

    pub fn bind(socket_path: PathBuf) -> io::Result<Self> {
        Self::bind_with_state(socket_path, Arc::new(Mutex::new(ScpState::new())))
    }

    pub fn bind_with_state(socket_path: PathBuf, state: Arc<Mutex<ScpState>>) -> io::Result<Self> {
        let listener = bind_listener(&socket_path)?;
        if let Err(error) =
            std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))
        {
            drop(listener);
            let _ = std::fs::remove_file(&socket_path);
            return Err(error);
        }
        listener.set_nonblocking(true)?;

        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = Arc::clone(&shutdown);
        let thread_state = Arc::clone(&state);
        let accept_thread = thread::Builder::new()
            .name("scp-listener".to_string())
            .spawn(move || accept_loop(listener, thread_state, thread_shutdown))?;

        tracing::info!(socket = %socket_path.display(), "SCP listener ready");
        Ok(Self {
            socket_path,
            state,
            shutdown,
            accept_thread: Some(accept_thread),
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

fn bind_listener(socket_path: &Path) -> io::Result<UnixListener> {
    match UnixListener::bind(socket_path) {
        Ok(listener) => Ok(listener),
        Err(bind_error) if bind_error.kind() == io::ErrorKind::AddrInUse => {
            match UnixStream::connect(socket_path) {
                Ok(_) => Err(bind_error),
                Err(connect_error)
                    if matches!(
                        connect_error.kind(),
                        io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
                    ) =>
                {
                    let metadata = std::fs::symlink_metadata(socket_path)?;
                    if !metadata.file_type().is_socket() {
                        return Err(io::Error::new(
                            io::ErrorKind::AlreadyExists,
                            "refusing to replace a non-socket SCP path",
                        ));
                    }
                    std::fs::remove_file(socket_path)?;
                    UnixListener::bind(socket_path)
                }
                Err(_) => Err(bind_error),
            }
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
        match std::fs::remove_file(&self.socket_path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                tracing::warn!(?error, path = %self.socket_path.display(), "failed to remove SCP socket")
            }
        }
    }
}

fn resolve_socket_path() -> io::Result<PathBuf> {
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

fn accept_loop(listener: UnixListener, state: Arc<Mutex<ScpState>>, shutdown: Arc<AtomicBool>) {
    while !shutdown.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                let state = Arc::clone(&state);
                let _ = thread::Builder::new()
                    .name("scp-client".to_string())
                    .spawn(move || {
                        if let Err(error) = serve_client(stream, &state) {
                            tracing::debug!(?error, "SCP client disconnected with error");
                        }
                    });
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
}

fn serve_client(mut stream: UnixStream, state: &Arc<Mutex<ScpState>>) -> io::Result<()> {
    let mut session_id = None;
    let mut bytes = Vec::new();
    let mut pending_fds = Vec::new();
    let result = serve_client_inner(
        &mut stream,
        state,
        &mut session_id,
        &mut bytes,
        &mut pending_fds,
    );

    close_all(&mut pending_fds);
    if let Some(session_id) = session_id
        && let Ok(mut state) = state.lock()
    {
        state.disconnect(session_id);
    }
    result
}

fn serve_client_inner(
    stream: &mut UnixStream,
    state: &Arc<Mutex<ScpState>>,
    session_id: &mut Option<SessionId>,
    bytes: &mut Vec<u8>,
    pending_fds: &mut Vec<i32>,
) -> io::Result<()> {
    let (peer_pid, _, _) = unix_socket::get_peer_credentials(stream.as_raw_fd())?;

    loop {
        if !receive_chunk(stream, bytes, pending_fds)? {
            break;
        }

        while let Some(payload) = take_frame(bytes)? {
            let message: ClientMessage = match serde_json::from_slice(&payload) {
                Ok(message) => message,
                Err(error) => {
                    close_all(pending_fds);
                    write_frame(
                        stream,
                        &CompositorMessage::ProtocolError {
                            code: "invalid-json".to_string(),
                            message: error.to_string(),
                            fatal: true,
                        },
                    )?;
                    return Ok(());
                }
            };

            if let ClientMessage::Connect { pid, .. } = &message
                && *pid != peer_pid
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

            let received_fd = match message {
                ClientMessage::AttachBuffer { .. } if pending_fds.len() == 1 => pending_fds.pop(),
                ClientMessage::AttachBuffer { .. } => {
                    let count = pending_fds.len();
                    close_all(pending_fds);
                    write_protocol_error(
                        stream,
                        "invalid-fd-count",
                        format!("AttachBuffer requires one descriptor, received {count}"),
                        false,
                    )?;
                    continue;
                }
                _ if pending_fds.is_empty() => None,
                _ => {
                    close_all(pending_fds);
                    write_protocol_error(
                        stream,
                        "unexpected-fd",
                        "descriptors are only accepted with AttachBuffer".to_string(),
                        false,
                    )?;
                    continue;
                }
            };

            let responses = state
                .lock()
                .map_err(|_| io::Error::other("SCP state lock poisoned"))?
                .handle_transport_message(*session_id, message, received_fd);

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
        }
    }

    Ok(())
}

fn receive_chunk(
    stream: &UnixStream,
    bytes: &mut Vec<u8>,
    pending_fds: &mut Vec<i32>,
) -> io::Result<bool> {
    let mut buffer = [0_u8; 64 * 1024];
    let (received, fds) = unix_socket::recvmsg_with_fds(stream.as_raw_fd(), &mut buffer, 4)?;

    if received == 0 {
        return Ok(false);
    }

    pending_fds.extend(fds);
    bytes.extend_from_slice(&buffer[..received]);
    Ok(true)
}

fn take_frame(bytes: &mut Vec<u8>) -> io::Result<Option<Vec<u8>>> {
    if bytes.len() < 4 {
        return Ok(None);
    }
    let frame_len = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    if frame_len == 0 || frame_len > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid SCP frame size {frame_len}"),
        ));
    }
    if bytes.len() < frame_len + 4 {
        return Ok(None);
    }
    let payload = bytes[4..frame_len + 4].to_vec();
    bytes.drain(..frame_len + 4);
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

pub fn write_frame<T: Serialize>(stream: &mut UnixStream, value: &T) -> io::Result<()> {
    let payload = serde_json::to_vec(value).map_err(io::Error::other)?;
    if payload.is_empty() || payload.len() > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "SCP frame is empty or too large",
        ));
    }
    let length = u32::try_from(payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "SCP frame is too large"))?;
    stream.write_all(&length.to_be_bytes())?;
    stream.write_all(&payload)
}

pub fn read_frame<T: DeserializeOwned>(stream: &mut UnixStream) -> io::Result<T> {
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
    serde_json::from_slice(&payload).map_err(io::Error::other)
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
}
