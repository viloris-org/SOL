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
    path::PathBuf,
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
}

impl Session {
    fn start() -> Session {
        // Build both binaries first (fail-fast in a clean tree).
        let compositor_bin = build_bin("sol-compositor");
        let _client_bin = build_bin("sol-compositor --example test-client");

        // Use a unique socket per test process to avoid clashing with any real
        // `wayland-sol` left running by hand.
        let socket = format!("{}-{}", SOCKET, std::process::id());

        let compositor = Command::new(compositor_bin)
            .env("SOL_WAYLAND_SOCKET", &socket)
            // Run the compositor in headless mode: no winit window, no GL.
            // This is the CI path — the protocol loop runs without any GPU /
            // display, so the test is deterministic on any runner.
            .arg("--headless")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn sol-compositor");

        Session { compositor, socket }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.compositor.kill();
        let _ = self.compositor.wait();
        // Best-effort cleanup of the socket file.
        let _ = std::fs::remove_file(socket_path(&self.socket));
    }
}

/// Path to the compositor socket under $XDG_RUNTIME_DIR.
fn socket_path(socket: &str) -> PathBuf {
    let run = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(run).join(socket)
}

/// Wait for the compositor's socket to exist (it binds before its event loop
/// starts accepting; polling on the file is a pragmatic readiness probe).
fn wait_for_socket(socket: &str, timeout: Duration) -> bool {
    let path = socket_path(socket);
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
    // `cargo build` the target first so we can execute it directly.
    let status = Command::new(env!("CARGO"))
        .args(["build", "--quiet", "-p", "sol-compositor"])
        .status()
        .expect("cargo build");
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

fn path_for_spec(tree: &PathBuf, spec: &str) -> PathBuf {
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
        wait_for_socket(&_session.socket, Duration::from_secs(10)),
        "compositor socket never appeared"
    );

    // Run the test client against our socket.
    let client_bin = build_bin("sol-compositor --example test-client");
    let output = Command::new(client_bin)
        .env("WAYLAND_DISPLAY", &_session.socket)
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
