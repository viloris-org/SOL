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
        .env("SOL_SETTINGSD_BIN", "/opt/sol/bin/sol-settingsd")
        .env("SOL_NOTIFICATIOND_BIN", "/opt/sol/bin/sol-notificationd")
        .env("SOL_PORTAL_BIN", "/opt/sol/bin/sol-portal")
        .output()
        .expect("run sol-session --dry-run");
    let _ = fs::remove_dir(&runtime_dir);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "compositor: /opt/sol/bin/sol-compositor --tty-udev\ncompositor env: XDG_RUNTIME_DIR="
            .to_owned()
            + &runtime_dir.display().to_string()
            + " SOL_WAYLAND_SOCKET=wayland-sol-test XDG_CURRENT_DESKTOP=SOL XDG_SESSION_DESKTOP=SOL\nsettingsd: /opt/sol/bin/sol-settingsd --dbus\nnotificationd: /opt/sol/bin/sol-notificationd --dbus\nportal: /opt/sol/bin/sol-portal --dbus\nshell: /opt/sol/bin/sol-shell\nshell env: XDG_RUNTIME_DIR="
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
fn actual_mode_starts_services_and_restarts_shell_until_compositor_exits() {
    let runtime_dir = runtime_dir();
    let compositor = executable(
        &runtime_dir,
        "fake-compositor",
        "#!/bin/sh\n: > \"$XDG_RUNTIME_DIR/$SOL_WAYLAND_SOCKET\"\nsleep 0.4\nexit 7\n",
    );
    let shell = executable(
        &runtime_dir,
        "fake-shell",
        "#!/bin/sh\ntest \"$WAYLAND_DISPLAY\" = wayland-sol-test\ntest \"$XDG_CURRENT_DESKTOP\" = SOL\ntest \"$XDG_SESSION_DESKTOP\" = SOL\ncount=0\n[ ! -f \"$XDG_RUNTIME_DIR/shell-count\" ] || count=$(cat \"$XDG_RUNTIME_DIR/shell-count\")\ncount=$((count + 1))\nprintf '%s\\n' \"$count\" > \"$XDG_RUNTIME_DIR/shell-count\"\n[ \"$count\" -ne 1 ] || exit 0\ntrap 'exit 0' TERM INT\nwhile :; do sleep 1; done\n",
    );
    let settingsd = service_executable(&runtime_dir, "fake-settingsd", "settingsd-started");
    let notificationd =
        service_executable(&runtime_dir, "fake-notificationd", "notificationd-started");
    let portal = service_executable(&runtime_dir, "fake-portal", "portal-started");
    let environment = sol_session::SessionEnvironment {
        runtime_dir: runtime_dir.clone(),
        socket: "wayland-sol-test".to_owned(),
    };
    let plan = sol_session::LaunchPlan::new(
        &environment,
        &sol_session::ProgramPaths {
            compositor,
            shell,
            settingsd,
            notificationd,
            portal,
        },
    );

    let result = sol_session::run(&plan);
    assert!(
        result
            .as_ref()
            .is_err_and(|error| error.contains("compositor exited")),
        "{result:?}"
    );
    assert_eq!(
        fs::read_to_string(runtime_dir.join("shell-count")).expect("read shell restart count"),
        "2\n"
    );
    for marker in [
        "settingsd-started",
        "notificationd-started",
        "portal-started",
    ] {
        assert!(runtime_dir.join(marker).exists(), "missing {marker}");
    }
    for entry in fs::read_dir(&runtime_dir).expect("read test runtime directory") {
        let path = entry.expect("read runtime entry").path();
        let _ = fs::remove_file(path);
    }
    let _ = fs::remove_dir(&runtime_dir);
}

fn service_executable(directory: &std::path::Path, name: &str, marker: &str) -> PathBuf {
    executable(
        directory,
        name,
        &format!(
            "#!/bin/sh\ntest \"$1\" = --dbus\ntest \"$XDG_CURRENT_DESKTOP\" = SOL\ntest \"$XDG_SESSION_DESKTOP\" = SOL\n: > \"$XDG_RUNTIME_DIR/{marker}\"\ntrap 'exit 0' TERM INT\nwhile :; do sleep 1; done\n"
        ),
    )
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
