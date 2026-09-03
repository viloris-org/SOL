//! Compositor-initiated events over a real socket.
//!
//! Request/reply already worked before per-session event queues existed: the
//! transport wrote whatever `handle_message` returned. What did not work was the
//! other direction — input, frame callbacks, popup dismissal — because those
//! originate on a *different* thread than the one serving the client, and often
//! target a different client entirely.
//!
//! These tests bind a real listener over a state handle the test also holds, so
//! events can be raised from the test thread while the client's transport thread
//! is blocked in `poll`. That is the path being verified: enqueue → eventfd →
//! wake → frame on the wire, including descriptor passing via SCM_RIGHTS.

// `expect` in a test is a deliberate assertion, not an unhandled error.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serial_test::serial;
use sol_compositor::scp::{
    ScpState,
    protocol::{
        CURRENT_PROTOCOL_VERSION, CaptureTarget, ClientMessage, CompositorMessage, CursorMode,
        InputEvent, SessionId,
    },
    transport::{ScpServer, write_frame},
    unix_socket,
    wire::WireMessage,
};
use std::{
    os::fd::{AsRawFd, RawFd},
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};

const READ_TIMEOUT: Duration = Duration::from_secs(5);
const OUTPUT_WIDTH: i32 = 1920;
const OUTPUT_HEIGHT: i32 = 1080;

/// A listener plus the state handle the test drives it through.
struct Harness {
    server: ScpServer,
    state: Arc<Mutex<ScpState>>,
}

impl Harness {
    fn start() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(1);
        let socket_path: PathBuf = std::env::temp_dir().join(format!(
            "sol-scp-events-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));

        let mut state = ScpState::new();
        state.output_manager_mut().add_output(
            "TEST-1".to_string(),
            "test output".to_string(),
            OUTPUT_WIDTH,
            OUTPUT_HEIGHT,
            60_000,
        );
        let state = Arc::new(Mutex::new(state));

        let server =
            ScpServer::bind_with_state(socket_path, Arc::clone(&state)).expect("bind SCP listener");
        Self { server, state }
    }

    fn connect(&self) -> Connection {
        let stream = UnixStream::connect(self.server.socket_path()).expect("connect to listener");
        stream
            .set_read_timeout(Some(READ_TIMEOUT))
            .expect("set read timeout");
        Connection {
            stream,
            buffer: Vec::new(),
            fds: Vec::new(),
        }
    }

    /// Run `action` against the shared state, as the input or render loop would.
    fn with_state<T>(&self, action: impl FnOnce(&mut ScpState) -> T) -> T {
        let mut guard = self.state.lock().expect("state lock");
        action(&mut guard)
    }
}

/// A client connection that keeps descriptors received alongside frames.
struct Connection {
    stream: UnixStream,
    buffer: Vec<u8>,
    fds: Vec<RawFd>,
}

impl Connection {
    fn send(&mut self, message: &ClientMessage) {
        write_frame(&mut self.stream, message).expect("send request");
    }

    /// Read the next frame, collecting any descriptor that arrives with it.
    ///
    /// Uses `recvmsg` rather than a plain read so SCM_RIGHTS payloads are not
    /// silently discarded — for a descriptor-bearing event that would look
    /// identical to success while dropping the whole point of the message.
    fn read_message(&mut self) -> CompositorMessage {
        let deadline = Instant::now() + READ_TIMEOUT;

        loop {
            if let Some(payload) = self.take_frame() {
                return CompositorMessage::decode_wire(&payload)
                    .expect("decode compositor message");
            }
            assert!(Instant::now() < deadline, "timed out waiting for a frame");

            let mut chunk = [0_u8; 64 * 1024];
            let (received, fds) =
                unix_socket::recvmsg_with_fds(self.stream.as_raw_fd(), &mut chunk, 4)
                    .expect("receive frame");
            assert_ne!(received, 0, "compositor closed the connection");
            self.fds.extend(fds);
            self.buffer.extend_from_slice(&chunk[..received]);
        }
    }

    /// Read frames until one satisfies `predicate`, returning it.
    fn read_until(
        &mut self,
        label: &str,
        predicate: impl Fn(&CompositorMessage) -> bool,
    ) -> CompositorMessage {
        for _ in 0..64 {
            let message = self.read_message();
            if predicate(&message) {
                return message;
            }
        }
        panic!("never received {label}");
    }

    fn take_frame(&mut self) -> Option<Vec<u8>> {
        if self.buffer.len() < 4 {
            return None;
        }
        let length = u32::from_be_bytes([
            self.buffer[0],
            self.buffer[1],
            self.buffer[2],
            self.buffer[3],
        ]) as usize;
        if self.buffer.len() < length + 4 {
            return None;
        }
        let payload = self.buffer[4..length + 4].to_vec();
        self.buffer.drain(..length + 4);
        Some(payload)
    }

    /// Authenticate and return the session id plus the toplevel capability token.
    fn authenticate(&mut self) -> (SessionId, Vec<u8>) {
        let app_id = std::fs::read_to_string(format!("/proc/{}/comm", std::process::id()))
            .expect("read process identity")
            .trim()
            .to_string();
        self.send(&ClientMessage::Connect {
            app_id,
            pid: std::process::id(),
        });

        match self.read_message() {
            CompositorMessage::Connected {
                session_id,
                capability_tokens,
                ..
            } => (session_id, capability_tokens["window-toplevel"].clone()),
            response => panic!("unexpected connect response: {response:?}"),
        }
    }

    fn authenticate_v2(&mut self) -> (SessionId, Vec<u8>) {
        let app_id = std::fs::read_to_string(format!("/proc/{}/comm", std::process::id()))
            .expect("read process identity")
            .trim()
            .to_string();
        self.send(&ClientMessage::ConnectVersioned {
            app_id,
            pid: std::process::id(),
            min_version: 2,
            max_version: 2,
        });
        assert!(matches!(
            self.read_message(),
            CompositorMessage::ProtocolVersion {
                version: CURRENT_PROTOCOL_VERSION,
                ..
            }
        ));
        match self.read_message() {
            CompositorMessage::Connected {
                session_id,
                capability_tokens,
                ..
            } => (session_id, capability_tokens["window-toplevel"].clone()),
            response => panic!("unexpected connect response: {response:?}"),
        }
    }

    /// Create a surface with a toplevel role and return its id.
    fn create_toplevel(&mut self, surface_id: u32, token: Vec<u8>) -> u32 {
        self.send(&ClientMessage::CreateSurface { surface_id });
        self.send(&ClientMessage::CreateToplevel {
            surface_id,
            capability_token: token,
            title: "event test".to_string(),
        });
        match self.read_until("ConfigureToplevel", |message| {
            matches!(message, CompositorMessage::ConfigureToplevel { .. })
        }) {
            CompositorMessage::ConfigureToplevel { toplevel_id, .. } => toplevel_id,
            response => panic!("unexpected response: {response:?}"),
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        for fd in self.fds.drain(..) {
            unix_socket::close_fd(fd);
        }
    }
}

#[test]
#[serial]
fn input_raised_on_another_thread_reaches_a_blocked_client() {
    let harness = Harness::start();
    let mut client = harness.connect();
    let (_session, token) = client.authenticate();
    client.create_toplevel(1, token);

    // The client is now idle in `poll`. Raise input from this thread, the way a
    // real input backend would, and it must arrive without the client asking.
    harness.with_state(|state| state.handle_pointer_motion(600.0, 300.0, 10));

    let message = client.read_until("PointerEnter", |message| {
        matches!(
            message,
            CompositorMessage::InputEvent {
                event: InputEvent::PointerEnter { .. },
                ..
            }
        )
    });

    match message {
        CompositorMessage::InputEvent {
            surface_id,
            event: InputEvent::PointerEnter { x, y, .. },
        } => {
            assert_eq!(surface_id, 1);
            // The window is centered at (560, 240), so (600, 300) is (40, 60) in.
            assert!((x - 40.0).abs() < f64::EPSILON, "local x was {x}");
            assert!((y - 60.0).abs() < f64::EPSILON, "local y was {y}");
        }
        response => panic!("unexpected response: {response:?}"),
    }
}

#[test]
#[serial]
fn a_pushed_keymap_carries_its_descriptor() {
    let harness = Harness::start();
    let mut client = harness.connect();
    let (_session, token) = client.authenticate();

    // Creating a toplevel takes keyboard focus, which publishes the keymap. The
    // keymap is useless without its memfd, so this is the case that proves the
    // outbound SCM_RIGHTS path works.
    client.create_toplevel(1, token);

    let message = client.read_until("KeymapFormat", |message| {
        matches!(message, CompositorMessage::KeymapFormat { .. })
    });

    let size = match message {
        CompositorMessage::KeymapFormat { size, .. } => size,
        response => panic!("unexpected response: {response:?}"),
    };
    assert!(size > 0, "the keymap must not be empty");
    assert_eq!(
        client.fds.len(),
        1,
        "the keymap descriptor must arrive out-of-band"
    );

    // The descriptor must be readable and hold exactly the advertised bytes.
    let mut keymap = Vec::new();
    let fd = client.fds.pop().expect("keymap descriptor");
    let mut file = unsafe {
        use std::os::unix::io::FromRawFd;
        std::fs::File::from_raw_fd(fd)
    };
    use std::io::Read;
    file.read_to_end(&mut keymap).expect("read keymap memfd");
    assert_eq!(keymap.len(), size as usize);
    assert!(
        keymap.starts_with(b"xkb_keymap"),
        "the memfd must contain an XKB keymap"
    );
}

#[test]
#[serial]
fn keyboard_focus_changes_reach_the_client_unprompted() {
    let harness = Harness::start();
    let mut client = harness.connect();
    let (session_id, token) = client.authenticate();
    client.create_toplevel(1, token);

    client.read_until("KeyboardEnter", |message| {
        matches!(
            message,
            CompositorMessage::InputEvent {
                event: InputEvent::KeyboardEnter { .. },
                ..
            }
        )
    });

    // Dropping focus from another thread must produce a leave event.
    harness.with_state(|state| {
        assert_eq!(state.get_focused_surface(), Some((session_id, 1)));
        state.set_keyboard_focus(None);
    });

    client.read_until("KeyboardLeave", |message| {
        matches!(
            message,
            CompositorMessage::InputEvent {
                event: InputEvent::KeyboardLeave { .. },
                ..
            }
        )
    });
}

#[test]
#[serial]
fn frame_callbacks_reach_the_client_after_a_render_pass() {
    let harness = Harness::start();
    let mut client = harness.connect();
    let (_session, token) = client.authenticate();
    client.create_toplevel(1, token);

    client.send(&ClientMessage::Commit {
        surface_id: 1,
        frame_callback: Some(99),
    });

    // Wait for the commit to be processed, then simulate a completed frame.
    let deadline = Instant::now() + READ_TIMEOUT;
    loop {
        let delivered = harness.with_state(|state| state.send_frame_callbacks(4_242));
        if delivered > 0 {
            break;
        }
        assert!(Instant::now() < deadline, "commit was never processed");
        std::thread::sleep(Duration::from_millis(10));
    }

    let message = client.read_until("FrameCallback", |message| {
        matches!(message, CompositorMessage::FrameCallback { .. })
    });
    match message {
        CompositorMessage::FrameCallback {
            surface_id,
            callback_id,
            timestamp_ms,
        } => {
            assert_eq!((surface_id, callback_id), (1, 99));
            assert_eq!(timestamp_ms, 4_242);
        }
        response => panic!("unexpected response: {response:?}"),
    }
}

#[test]
#[serial]
fn a_compositor_initiated_close_reaches_the_client() {
    let harness = Harness::start();
    let mut client = harness.connect();
    let (_session, token) = client.authenticate();
    let toplevel_id = client.create_toplevel(1, token);

    // The titlebar close button lives in the compositor, so closing a window is
    // something the client learns about rather than requests.
    harness.with_state(|state| state.close_toplevel(toplevel_id));

    let message = client.read_until("ToplevelClosed", |message| {
        matches!(message, CompositorMessage::ToplevelClosed { .. })
    });
    match message {
        CompositorMessage::ToplevelClosed {
            toplevel_id: closed,
        } => {
            assert_eq!(closed, toplevel_id);
        }
        response => panic!("unexpected response: {response:?}"),
    }
}

#[test]
#[serial]
fn events_reach_the_right_client_among_several() {
    let harness = Harness::start();

    let mut first = harness.connect();
    let (first_session, first_token) = first.authenticate();
    first.create_toplevel(1, first_token);

    let mut second = harness.connect();
    let (second_session, second_token) = second.authenticate();
    second.create_toplevel(1, second_token);

    assert_ne!(first_session, second_session);

    // The newest window is on top and holds focus, so a key goes to the second
    // client and must not leak to the first.
    harness.with_state(|state| {
        state.handle_key(30, sol_compositor::scp::protocol::KeyState::Pressed, 10);
    });

    second.read_until("KeyboardKey", |message| {
        matches!(
            message,
            CompositorMessage::InputEvent {
                event: InputEvent::KeyboardKey { key: 30, .. },
                ..
            }
        )
    });

    // Drain whatever the first client legitimately has queued and confirm none
    // of it is the keystroke that belonged to the other window.
    first
        .stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .expect("shorten timeout");
    let mut saw_key = false;
    while let Some(payload) = first
        .take_frame()
        .or_else(|| read_available(&mut first).and_then(|()| first.take_frame()))
    {
        let message = CompositorMessage::decode_wire(&payload).expect("decode compositor message");
        if matches!(
            message,
            CompositorMessage::InputEvent {
                event: InputEvent::KeyboardKey { .. },
                ..
            }
        ) {
            saw_key = true;
        }
    }
    assert!(
        !saw_key,
        "a keystroke must only reach the focused client's window"
    );
}

#[test]
#[serial]
fn v2_capture_delivers_a_sealed_rgba_frame_over_scm_rights() {
    use std::{io::Read, os::fd::FromRawFd};

    let harness = Harness::start();
    let mut client = harness.connect();
    let (_session, token) = client.authenticate_v2();
    client.create_toplevel(1, token);

    client.send(&ClientMessage::RequestCapability {
        capability: "screen-capture-output".to_string(),
        justification: "integration test".to_string(),
    });
    let token = match client.read_until("capture capability", |message| {
        matches!(
            message,
            CompositorMessage::CapabilityDecision {
                capability,
                ..
            } if capability == "screen-capture-output"
        )
    }) {
        CompositorMessage::CapabilityDecision {
            granted: true,
            token: Some(token),
            ..
        } => token,
        response => panic!("capture was not granted: {response:?}"),
    };

    // Creating the focused toplevel also publishes a keymap descriptor. It is
    // unrelated to this assertion, so release it before requesting the frame.
    for fd in client.fds.drain(..) {
        unix_socket::close_fd(fd);
    }
    client.send(&ClientMessage::RequestCapture {
        target: CaptureTarget::Output(0),
        cursor_mode: CursorMode::Exclude,
        capability_token: token,
    });
    let message = client.read_until("CaptureGranted", |message| {
        matches!(message, CompositorMessage::CaptureGranted { .. })
    });
    let (width, height, stride) = match message {
        CompositorMessage::CaptureGranted {
            width,
            height,
            stride,
            ..
        } => (width, height, stride),
        response => panic!("unexpected capture response: {response:?}"),
    };
    assert_eq!((width, height), (OUTPUT_WIDTH as u32, OUTPUT_HEIGHT as u32));
    assert_eq!(stride, width * 4);
    assert_eq!(client.fds.len(), 1, "capture must carry one descriptor");

    let fd = client.fds.pop().expect("capture descriptor");
    let seals = sol_compositor::scp::memfd::seals(fd).expect("capture memfd seals");
    assert_ne!(seals & sol_compositor::scp::memfd::F_SEAL_WRITE, 0);
    let mut bytes = Vec::new();
    // SAFETY: the descriptor was removed from Connection and is now owned here.
    unsafe { std::fs::File::from_raw_fd(fd) }
        .read_to_end(&mut bytes)
        .expect("read capture pixels");
    assert_eq!(bytes.len(), stride as usize * height as usize);
}

/// Pull whatever is immediately available into the connection's buffer.
///
/// Returns `None` once the socket is quiet, which ends the caller's drain loop.
fn read_available(connection: &mut Connection) -> Option<()> {
    let mut chunk = [0_u8; 64 * 1024];
    match unix_socket::recvmsg_with_fds(connection.stream.as_raw_fd(), &mut chunk, 4) {
        Ok((0, _)) => None,
        Ok((received, fds)) => {
            connection.fds.extend(fds);
            connection.buffer.extend_from_slice(&chunk[..received]);
            Some(())
        }
        Err(_) => None,
    }
}
