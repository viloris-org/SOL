use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

fn runtime_dir() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "sol-session-test-{}-{}",
        std::process::id(),
        NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).expect("create runtime directory");
    path
}

#[test]
fn binary_prints_a_non_hardware_launch_plan() {
    let runtime_dir = runtime_dir();
    let output = Command::new(env!("CARGO_BIN_EXE_sol-session"))
        .args(["--dry-run", "--socket", "wayland-sol-test"])
        .env("XDG_RUNTIME_DIR", &runtime_dir)
        .env("SOL_COMPOSITOR_BIN", "/opt/sol/bin/sol-compositor")
        .env("SOL_SHELL_BIN", "/opt/sol/bin/sol-shell")
        .output()
        .expect("run sol-session --dry-run");
    let _ = fs::remove_dir(&runtime_dir);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "compositor: /opt/sol/bin/sol-compositor --tty-udev\ncompositor env: XDG_RUNTIME_DIR="
            .to_owned()
            + &runtime_dir.display().to_string()
            + " SOL_WAYLAND_SOCKET=wayland-sol-test XDG_CURRENT_DESKTOP=SOL XDG_SESSION_DESKTOP=SOL\nshell: /opt/sol/bin/sol-shell\nshell env: XDG_RUNTIME_DIR="
            + &runtime_dir.display().to_string()
            + " WAYLAND_DISPLAY=wayland-sol-test XDG_CURRENT_DESKTOP=SOL XDG_SESSION_DESKTOP=SOL\nwait for socket: "
            + &runtime_dir.join("wayland-sol-test").display().to_string()
            + "\n"
    );
}

#[test]
fn binary_rejects_socket_paths_before_starting_processes() {
    let runtime_dir = runtime_dir();
    let output = Command::new(env!("CARGO_BIN_EXE_sol-session"))
        .args(["--dry-run", "--socket", "not/a-socket"])
        .env("XDG_RUNTIME_DIR", &runtime_dir)
        .output()
        .expect("run sol-session with invalid socket");
    let _ = fs::remove_dir(&runtime_dir);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("not a path"));
}

#[test]
fn actual_mode_orders_processes_and_stops_the_compositor_when_shell_exits() {
    let runtime_dir = runtime_dir();
    let compositor = executable(
        &runtime_dir,
        "fake-compositor",
        "#!/bin/sh\n: > \"$XDG_RUNTIME_DIR/$SOL_WAYLAND_SOCKET\"\ntrap 'exit 0' TERM INT\nwhile :; do sleep 1; done\n",
    );
    let shell = executable(
        &runtime_dir,
        "fake-shell",
        "#!/bin/sh\ntest \"$WAYLAND_DISPLAY\" = wayland-sol-test\ntest \"$XDG_CURRENT_DESKTOP\" = SOL\ntest \"$XDG_SESSION_DESKTOP\" = SOL\ntest -d \"$XDG_RUNTIME_DIR\"\nexit 0\n",
    );
    let environment = sol_session::SessionEnvironment {
        runtime_dir: runtime_dir.clone(),
        socket: "wayland-sol-test".to_owned(),
    };
    let plan = sol_session::LaunchPlan::new(
        &environment,
        &sol_session::ProgramPaths { compositor, shell },
    );

    let result = sol_session::run(&plan);
    let _ = fs::remove_file(runtime_dir.join("wayland-sol-test"));
    let _ = fs::remove_file(runtime_dir.join("fake-compositor"));
    let _ = fs::remove_file(runtime_dir.join("fake-shell"));
    let _ = fs::remove_dir(&runtime_dir);
    assert!(result.is_ok(), "{result:?}");
}

fn executable(directory: &std::path::Path, name: &str, body: &str) -> PathBuf {
    let path = directory.join(name);
    fs::write(&path, body).expect("write test executable");
    let mut permissions = fs::metadata(&path)
        .expect("stat test executable")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).expect("make test executable executable");
    path
}
