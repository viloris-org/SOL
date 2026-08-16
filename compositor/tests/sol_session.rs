//! SOL integration test: start the compositor, drive a real Wayland client
//! against it, and assert the end-to-end session works.
//!
//! This proves the PRD §38 Phase 0 success criterion:
//!
//! > 能启动独立 SOL Wayland Session，并运行标准 Wayland 应用
//!
//! The compositor runs on the winit backend in this environment; the client is
//! the example built into `sol-compositor` (`test-client`). We:
//!   1. launch `sol-compositor`,
//!   2. wait until its `wayland-sol` socket appears,
//!   3. run `test-client` against it,
//!   4. assert the client connected and its toplevel was acknowledged (a
//!      protocol round-trip through the compositor's dispatch + render loop).

use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use serial_test::serial;

const SOCKET: &str = "wayland-sol";

/// Spawn the compositor with a unique `SOL_WAYLAND_SOCKET` so parallel runs
/// do not collide, and return the socket path that a client should connect to.
struct Session {
    compositor: Child,
    socket: String,
    runtime_dir: PathBuf,
}

impl Session {
    fn start() -> Session {
        Self::start_with_scale(None)
    }

    fn start_with_scale(scale: Option<&str>) -> Session {
        // Build both binaries first (fail-fast in a clean tree).
        let compositor_bin = build_bin("sol-compositor");
        let _client_bin = build_bin("sol-compositor --example test-client");

        // Use a unique socket per test process to avoid clashing with any real
        // `wayland-sol` left running by hand.
        let socket = format!("{}-{}", SOCKET, std::process::id());
        let runtime_dir = std::env::temp_dir().join(format!("sol-session-{socket}"));
        std::fs::create_dir_all(&runtime_dir).expect("create isolated Wayland runtime directory");

        let mut compositor_command = Command::new(compositor_bin);
        compositor_command
            .env("SOL_WAYLAND_SOCKET", &socket)
            .env("XDG_RUNTIME_DIR", &runtime_dir)
            // Run the compositor in headless mode: no winit window, no GL.
            // This is the CI path — the protocol loop runs without any GPU /
            // display, so the test is deterministic on any runner.
            .arg("--headless");
        if let Some(scale) = scale {
            compositor_command.env("SOL_OUTPUT_SCALE", scale);
        }
        let compositor = compositor_command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn sol-compositor");

        Session {
            compositor,
            socket,
            runtime_dir,
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.compositor.kill();
        let _ = self.compositor.wait();
        // Best-effort cleanup of the socket file.
        let _ = std::fs::remove_file(self.socket_path());
        let _ = std::fs::remove_file(self.socket_path().with_extension("lock"));
        let _ = std::fs::remove_dir(&self.runtime_dir);
    }
}

impl Session {
    fn socket_path(&self) -> PathBuf {
        self.runtime_dir.join(&self.socket)
    }
}

/// Wait for the compositor's socket to exist (it binds before its event loop
/// starts accepting; polling on the file is a pragmatic readiness probe).
fn wait_for_socket(session: &Session, timeout: Duration) -> bool {
    let path = session.socket_path();
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

fn build_bin(spec: &str) -> PathBuf {
    // Build the exact executable requested so examples are available even in
    // a clean target directory.
    let mut command = Command::new(env!("CARGO"));
    command.args(["build", "--quiet"]);
    if let Some((_, rest)) = spec.split_once("--example ") {
        command.args(["-p", "sol-compositor", "--example", rest.trim()]);
    } else {
        command.args(["-p", "sol-compositor", "-p", "sol-shell"]);
    }
    let status = command.status().expect("cargo build");
    assert!(status.success(), "cargo build failed for {spec}");

    // Workspace target dir sits at the workspace root (../../target from the
    // compositor crate), not per-crate.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let tree = workspace_root.join("target").join("debug");
    path_for_spec(&tree, spec)
}

fn path_for_spec(tree: &Path, spec: &str) -> PathBuf {
    if let Some((base, rest)) = spec.split_once(' ') {
        // "sol-compositor --example test-client" -> target/debug/examples/test-client
        if rest.contains("--example") {
            let name = rest.split_whitespace().last().unwrap();
            return tree.join("examples").join(name);
        }
        let _ = base;
    }
    // Bare spec resolves to target/debug/sol-compositor.
    tree.join(spec.split_whitespace().next().unwrap())
}

#[test]
#[serial]
fn client_round_trip_against_compositor() {
    let _session = Session::start();
    assert!(
        wait_for_socket(&_session, Duration::from_secs(10)),
        "compositor socket never appeared"
    );

    // Run the test client against our socket.
    let client_bin = build_bin("sol-compositor --example test-client");
    let output = Command::new(client_bin)
        .env("WAYLAND_DISPLAY", &_session.socket)
        .env("XDG_RUNTIME_DIR", &_session.runtime_dir)
        .output()
        .expect("run test-client");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    eprintln!("client stdout: {stdout}");
    eprintln!("client stderr: {stderr}");
    assert!(
        output.status.success(),
        "test-client failed with {:?}",
        output.status
    );
    // The client reports success on stderr (eprintln), but accept either.
    assert!(
        stderr.contains("success") || stdout.contains("success"),
        "test-client did not report a successful round-trip"
    );
}

#[test]
#[serial]
fn fractional_scale_round_trip_against_compositor() {
    let session = Session::start_with_scale(Some("1.25"));
    assert!(
        wait_for_socket(&session, Duration::from_secs(10)),
        "compositor socket never appeared"
    );

    let client_bin = build_bin("sol-compositor --example fractional-scale-client");
    let output = Command::new(client_bin)
        .env("WAYLAND_DISPLAY", &session.socket)
        .env("XDG_RUNTIME_DIR", &session.runtime_dir)
        .output()
        .expect("run fractional-scale-client");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "fractional-scale-client failed with {:?}: {stderr}",
        output.status
    );
    assert!(
        stderr.contains("preferred=150"),
        "fractional-scale-client did not observe 1.25x (150/120): {stderr}"
    );
}

/// Prove the layer-shell top bar round-trip (Roadmap Phase 1 M1).
///
/// The compositor advertises `zwlr_layer_shell_v1`; the shell binds it, creates
/// a layer surface, receives a Configure, renders a frame, and exits 0 in
/// `--once` mode. This validates that the shell top bar and the compositor
/// coexist over the layer-shell protocol without crashing either side.
#[test]
#[serial]
fn shell_top_bar_round_trip_against_compositor() {
    let _session = Session::start();
    assert!(
        wait_for_socket(&_session, Duration::from_secs(10)),
        "compositor socket never appeared"
    );

    let shell_bin = build_bin("sol-shell");
    let output = Command::new(shell_bin)
        .env("WAYLAND_DISPLAY", &_session.socket)
        .env("XDG_RUNTIME_DIR", &_session.runtime_dir)
        .arg("--once")
        .output()
        .expect("run sol-shell --once");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    eprintln!("shell stdout: {stdout}");
    eprintln!("shell stderr: {stderr}");
    assert!(
        output.status.success(),
        "sol-shell --once failed with {:?}\nstdout: {stdout}\nstderr: {stderr}",
        output.status
    );
    assert!(
        stdout.contains("round-trip OK") || stderr.contains("round-trip OK"),
        "sol-shell did not report a successful layer-surface round-trip"
    );
}

/// Prove the standard Wayland data-device clipboard path in an isolated
/// compositor session. This does not touch the user's live desktop clipboard.
#[test]
#[serial]
fn clipboard_selection_round_trip_against_compositor() {
    let session = Session::start();
    assert!(
        wait_for_socket(&session, Duration::from_secs(10)),
        "compositor socket never appeared"
    );

    let client_bin = build_bin("sol-compositor --example clipboard-client");
    let output = Command::new(client_bin)
        .env("WAYLAND_DISPLAY", &session.socket)
        .env("XDG_RUNTIME_DIR", &session.runtime_dir)
        .output()
        .expect("run clipboard-client");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    eprintln!("clipboard stdout: {stdout}");
    eprintln!("clipboard stderr: {stderr}");
    assert!(
        output.status.success(),
        "clipboard-client failed with {:?}\nstdout: {stdout}\nstderr: {stderr}",
        output.status
    );
    assert!(
        stdout.contains("clipboard selection round-trip completed")
            || stderr.contains("clipboard selection round-trip completed"),
        "clipboard-client did not report a successful selection transfer"
    );
}
