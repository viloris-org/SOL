//! End-to-end native SCP transport and security checks.

// `expect` in a test is a deliberate assertion, not an unhandled error.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serial_test::serial;
use sol_compositor::scp::{
    memfd,
    protocol::{BufferFormat, ClientMessage, CompositorMessage},
    transport::{read_frame, write_frame, write_frame_with_fd},
};
use std::{
    fs::File,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::net::UnixStream,
    },
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Session {
    compositor: Child,
    runtime_dir: PathBuf,
    scp_socket: PathBuf,
}

impl Session {
    fn start() -> Self {
        let runtime_dir = std::env::temp_dir().join(format!(
            "sol-scp-session-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&runtime_dir).expect("create isolated runtime directory");
        let socket_name = format!("scp-{}", std::process::id());
        let scp_socket = runtime_dir.join(&socket_name);
        let compositor = Command::new(env!("CARGO_BIN_EXE_sol-compositor"))
            .env("XDG_RUNTIME_DIR", &runtime_dir)
            .env("SOL_SCP_SOCKET", &socket_name)
            // This fixture exercises framing, credentials and token rejection
            // in a subprocess. Production omits this explicit unsafe switch and
            // always coordinates with the separately supervised securityd.
            .env("SOL_SCP_INSECURE_STUB_SECURITY", "1")
            .env(
                "SOL_COMPOSITOR_SOCKET",
                format!("sol-{}", std::process::id()),
            )
            .arg("--headless")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn compositor");
        Self {
            compositor,
            runtime_dir,
            scp_socket,
        }
    }

    fn connect(&self) -> UnixStream {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if let Ok(stream) = UnixStream::connect(&self.scp_socket) {
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .expect("set timeout");
                return stream;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("SCP socket never became ready");
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.compositor.kill();
        let _ = self.compositor.wait();
        let _ = std::fs::remove_file(&self.scp_socket);
        let _ = std::fs::remove_dir_all(&self.runtime_dir);
    }
}

fn authenticate(stream: &mut UnixStream) -> Vec<u8> {
    let app_id = std::fs::read_to_string(format!("/proc/{}/comm", std::process::id()))
        .expect("read process identity")
        .trim()
        .to_string();
    write_frame(
        stream,
        &ClientMessage::Connect {
            app_id,
            pid: std::process::id(),
        },
    )
    .expect("send connect");
    match read_frame::<CompositorMessage>(stream).expect("read connected") {
        CompositorMessage::Connected {
            capability_tokens, ..
        } => capability_tokens["window-toplevel"].clone(),
        response => panic!("unexpected response: {response:?}"),
    }
}

#[test]
#[serial]
fn native_toplevel_and_forgery_rejection() {
    let session = Session::start();
    let mut stream = session.connect();
    let token = authenticate(&mut stream);

    write_frame(&mut stream, &ClientMessage::CreateSurface { surface_id: 1 })
        .expect("create surface");
    write_frame(
        &mut stream,
        &ClientMessage::CreateToplevel {
            surface_id: 1,
            capability_token: b"forged".to_vec(),
            title: "forged".to_string(),
        },
    )
    .expect("send forged request");
    assert!(matches!(
        read_frame::<CompositorMessage>(&mut stream).expect("read rejection"),
        CompositorMessage::ProtocolError { fatal: false, .. }
    ));

    write_frame(
        &mut stream,
        &ClientMessage::CreateToplevel {
            surface_id: 1,
            capability_token: token,
            title: "native".to_string(),
        },
    )
    .expect("send valid request");
    assert!(matches!(
        read_frame::<CompositorMessage>(&mut stream).expect("read configure"),
        CompositorMessage::ConfigureToplevel {
            decoration_height: 32,
            ..
        }
    ));
}

#[test]
#[serial]
fn buffer_fd_arrives_via_scm_rights() {
    let session = Session::start();
    let mut stream = session.connect();
    let _token = authenticate(&mut stream);
    write_frame(&mut stream, &ClientMessage::CreateSurface { surface_id: 2 })
        .expect("create surface");

    let message = ClientMessage::AttachBuffer {
        surface_id: 2,
        buffer_fd: -1,
        width: 16,
        height: 16,
        stride: 64,
        format: BufferFormat::Argb8888,
    };
    // 16 rows at stride 64 is 1024 bytes, sealed so it cannot shrink afterwards.
    send_with_descriptor(&mut stream, &message, sealed_memfd(1024));

    write_frame(
        &mut stream,
        &ClientMessage::RequestCapability {
            capability: "not-a-capability".to_string(),
            justification: "transport synchronization".to_string(),
        },
    )
    .expect("send synchronization request");
    // The attach is silent on success, so the next reply proves it was accepted:
    // a rejected attach would answer with a ProtocolError first.
    assert!(matches!(
        read_frame::<CompositorMessage>(&mut stream).expect("read decision"),
        CompositorMessage::CapabilityDecision { granted: false, .. }
    ));
}

#[test]
#[serial]
fn a_buffer_descriptor_that_can_shrink_is_refused() {
    let session = Session::start();
    let mut stream = session.connect();
    let _token = authenticate(&mut stream);
    write_frame(&mut stream, &ClientMessage::CreateSurface { surface_id: 2 })
        .expect("create surface");

    // Right size, no seal: the client could ftruncate it after this check and
    // leave the compositor reading pages that no longer exist.
    let message = ClientMessage::AttachBuffer {
        surface_id: 2,
        buffer_fd: -1,
        width: 16,
        height: 16,
        stride: 64,
        format: BufferFormat::Argb8888,
    };
    send_with_descriptor(&mut stream, &message, unsealed_memfd(1024));

    match read_frame::<CompositorMessage>(&mut stream).expect("read rejection") {
        CompositorMessage::ProtocolError { message, fatal, .. } => {
            assert!(!fatal, "a bad buffer is the client's mistake, not a kill");
            assert!(message.contains("F_SEAL_SHRINK"), "unexpected: {message}");
        }
        other => panic!("an unsealed buffer must be refused: {other:?}"),
    }
}

#[test]
#[serial]
fn a_buffer_descriptor_smaller_than_its_geometry_is_refused() {
    let session = Session::start();
    let mut stream = session.connect();
    let _token = authenticate(&mut stream);
    write_frame(&mut stream, &ClientMessage::CreateSurface { surface_id: 2 })
        .expect("create surface");

    // Geometry says 1024 bytes; the descriptor backs 512.
    let message = ClientMessage::AttachBuffer {
        surface_id: 2,
        buffer_fd: -1,
        width: 16,
        height: 16,
        stride: 64,
        format: BufferFormat::Argb8888,
    };
    send_with_descriptor(&mut stream, &message, sealed_memfd(512));

    match read_frame::<CompositorMessage>(&mut stream).expect("read rejection") {
        CompositorMessage::ProtocolError { message, .. } => {
            assert!(
                message.contains("exceeds the descriptor"),
                "unexpected: {message}"
            );
        }
        other => panic!("an undersized buffer must be refused: {other:?}"),
    }
}

/// Send one framed message with a descriptor attached via SCM_RIGHTS.
fn send_with_descriptor(stream: &mut UnixStream, message: &ClientMessage, fd: OwnedFd) {
    write_frame_with_fd(stream, message, fd.as_raw_fd()).expect("send descriptor");
}

/// A memfd of exactly `bytes` length that can still be shrunk.
fn unsealed_memfd(bytes: usize) -> OwnedFd {
    use std::io::Write;

    let fd = memfd::create("scp-test-buffer", true).expect("create memfd");
    // SAFETY: create returned a fresh owned descriptor nothing else holds.
    let mut file = unsafe { File::from_raw_fd(fd) };
    file.write_all(&vec![0_u8; bytes]).expect("size the memfd");
    OwnedFd::from(file)
}

/// A memfd of exactly `bytes` length, sealed against shrinking.
fn sealed_memfd(bytes: usize) -> OwnedFd {
    let fd = unsealed_memfd(bytes);
    memfd::add_seals(fd.as_raw_fd(), memfd::F_SEAL_SHRINK).expect("seal the memfd");
    fd
}
