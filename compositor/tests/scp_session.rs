//! End-to-end native SCP transport and security checks.

use nix::sys::socket::{ControlMessage, MsgFlags, sendmsg};
use serial_test::serial;
use sol_compositor::scp::{
    protocol::{BufferFormat, ClientMessage, CompositorMessage},
    transport::{read_frame, write_frame},
};
use std::{
    io::IoSlice,
    os::{fd::AsRawFd, unix::net::UnixStream},
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
            .env(
                "SOL_WAYLAND_SOCKET",
                format!("wayland-{}", std::process::id()),
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
    let payload = serde_json::to_vec(&message).expect("serialize attach");
    let mut frame = Vec::with_capacity(payload.len() + 4);
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&payload);
    let (buffer, _peer) = UnixStream::pair().expect("create descriptor fixture");
    let fds = [buffer.as_raw_fd()];
    sendmsg::<()>(
        stream.as_raw_fd(),
        &[IoSlice::new(&frame)],
        &[ControlMessage::ScmRights(&fds)],
        MsgFlags::empty(),
        None,
    )
    .expect("send buffer descriptor");

    write_frame(
        &mut stream,
        &ClientMessage::RequestCapability {
            capability: "not-a-capability".to_string(),
            justification: "transport synchronization".to_string(),
        },
    )
    .expect("send synchronization request");
    assert!(matches!(
        read_frame::<CompositorMessage>(&mut stream).expect("read decision"),
        CompositorMessage::CapabilityDecision { granted: false, .. }
    ));
}
