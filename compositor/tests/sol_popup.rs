//! A-2: xdg_surface popup round-trip.
//!
//! Creates a toplevel + a popup on it, verifies the compositor delivers
//! configure events for both, and exits 0 on success.
//!
//! Verifies: the compositor handles xdg popup lifecycle correctly (A-2).

use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use serial_test::serial;

const SOCKET: &str = "sol-0";

struct Session {
    compositor: Child,
    socket: String,
    runtime_dir: PathBuf,
}

impl Session {
    fn start() -> Session {
        let compositor_bin = build_bin("sol-compositor");
        let socket = format!("{}-{}", SOCKET, std::process::id());
        let runtime_dir = std::env::temp_dir().join(format!("sol-session-{socket}"));
        std::fs::create_dir_all(&runtime_dir).expect("create isolated runtime directory");

        let compositor = Command::new(compositor_bin)
            .env("SOL_COMPOSITOR_SOCKET", &socket)
            .env("XDG_RUNTIME_DIR", &runtime_dir)
            .arg("--headless")
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
    let mut command = Command::new(env!("CARGO"));
    command.args(["build", "--quiet"]);
    if let Some((_, rest)) = spec.split_once("--example ") {
        command.args(["-p", "sol-compositor", "--example", rest.trim()]);
    } else {
        command.args(["-p", "sol-compositor"]);
    }
    let status = command.status().expect("cargo build");
    assert!(status.success(), "cargo build failed for {spec}");

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let tree = workspace_root.join("target").join("debug");
    if let Some((_, rest)) = spec.split_once(' ') {
        if rest.contains("--example") {
            let name = rest.split_whitespace().last().unwrap();
            return tree.join("examples").join(name);
        }
    }
    tree.join(spec.split_whitespace().next().unwrap())
}

#[test]
#[serial]
fn popup_round_trip() {
    let _session = Session::start();
    assert!(
        wait_for_socket(&_session, Duration::from_secs(10)),
        "compositor socket never appeared"
    );

    let client_bin = build_bin("sol-compositor --example popup-client");
    let output = Command::new(client_bin)
        .env("WAYLAND_DISPLAY", &_session.socket)
        .env("XDG_RUNTIME_DIR", &_session.runtime_dir)
        .output()
        .expect("run popup-client");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    eprintln!("popup stdout: {stdout}");
    eprintln!("popup stderr: {stderr}");
    assert!(
        output.status.success(),
        "popup-client failed with {:?}",
        output.status
    );
    assert!(
        stderr.contains("popup round-trip OK") || stdout.contains("popup round-trip OK"),
        "popup-client did not report successful popup round-trip"
    );
}
