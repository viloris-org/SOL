//! Session-launch contract for an installed SOL desktop.
//!
//! This crate deliberately owns process ordering and environment propagation,
//! not seat/login management. The compositor remains responsible for opening a
//! real DRM/libseat session when its `--tty-udev` backend is selected.

use std::{
    env,
    ffi::OsString,
    fs::{self, OpenOptions},
    io,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const DEFAULT_SOCKET: &str = "wayland-sol";
const READY_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionEnvironment {
    pub runtime_dir: PathBuf,
    pub socket: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramPaths {
    pub compositor: PathBuf,
    pub shell: PathBuf,
}

impl ProgramPaths {
    #[must_use]
    pub fn from_environment() -> Self {
        Self {
            compositor: env_path("SOL_COMPOSITOR_BIN", "sol-compositor"),
            shell: env_path("SOL_SHELL_BIN", "sol-shell"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessPlan {
    pub program: PathBuf,
    pub arguments: Vec<OsString>,
    pub environment: Vec<(OsString, OsString)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlan {
    pub compositor: ProcessPlan,
    pub shell: ProcessPlan,
    pub socket_path: PathBuf,
}

impl LaunchPlan {
    #[must_use]
    pub fn new(environment: &SessionEnvironment, programs: &ProgramPaths) -> Self {
        let runtime_dir = environment.runtime_dir.as_os_str().to_os_string();
        let socket = OsString::from(&environment.socket);
        Self {
            compositor: ProcessPlan {
                program: programs.compositor.clone(),
                arguments: vec![OsString::from("--tty-udev")],
                environment: vec![
                    (OsString::from("XDG_RUNTIME_DIR"), runtime_dir.clone()),
                    (OsString::from("SOL_WAYLAND_SOCKET"), socket.clone()),
                    (OsString::from("XDG_CURRENT_DESKTOP"), OsString::from("SOL")),
                    (OsString::from("XDG_SESSION_DESKTOP"), OsString::from("SOL")),
                ],
            },
            shell: ProcessPlan {
                program: programs.shell.clone(),
                arguments: Vec::new(),
                environment: vec![
                    (OsString::from("XDG_RUNTIME_DIR"), runtime_dir),
                    (OsString::from("WAYLAND_DISPLAY"), socket),
                    (OsString::from("XDG_CURRENT_DESKTOP"), OsString::from("SOL")),
                    (OsString::from("XDG_SESSION_DESKTOP"), OsString::from("SOL")),
                ],
            },
            socket_path: environment.runtime_dir.join(&environment.socket),
        }
    }

    #[must_use]
    pub fn dry_run_output(&self) -> String {
        format!(
            "compositor: {} --tty-udev\ncompositor env: XDG_RUNTIME_DIR={} SOL_WAYLAND_SOCKET={} XDG_CURRENT_DESKTOP={} XDG_SESSION_DESKTOP={}\nshell: {}\nshell env: XDG_RUNTIME_DIR={} WAYLAND_DISPLAY={} XDG_CURRENT_DESKTOP={} XDG_SESSION_DESKTOP={}\nwait for socket: {}\n",
            self.compositor.program.display(),
            value(&self.compositor.environment[0].1),
            value(&self.compositor.environment[1].1),
            value(&self.compositor.environment[2].1),
            value(&self.compositor.environment[3].1),
            self.shell.program.display(),
            value(&self.shell.environment[0].1),
            value(&self.shell.environment[1].1),
            value(&self.shell.environment[2].1),
            value(&self.shell.environment[3].1),
            self.socket_path.display(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cli {
    pub dry_run: bool,
    pub socket_override: Option<String>,
}

pub fn parse_cli(arguments: impl IntoIterator<Item = String>) -> Result<Cli, String> {
    let mut dry_run = false;
    let mut socket_override = None;
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--dry-run" => dry_run = true,
            "--socket" => {
                let socket = arguments
                    .next()
                    .ok_or_else(|| "--socket requires a socket name".to_owned())?;
                socket_override = Some(socket);
            }
            "--help" | "-h" => return Err(usage()),
            _ => return Err(format!("unknown argument: {argument}\n{}", usage())),
        }
    }
    Ok(Cli {
        dry_run,
        socket_override,
    })
}

#[must_use]
pub fn usage() -> String {
    "Usage: sol-session [--dry-run] [--socket NAME]".to_owned()
}

pub fn environment(socket_override: Option<String>) -> Result<SessionEnvironment, String> {
    let runtime_dir = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| "XDG_RUNTIME_DIR must be set for a SOL session".to_owned())?;
    validate_runtime_dir(&runtime_dir)?;
    let socket = socket_override
        .or_else(|| env::var("SOL_WAYLAND_SOCKET").ok())
        .unwrap_or_else(|| DEFAULT_SOCKET.to_owned());
    validate_socket_name(&socket)?;
    Ok(SessionEnvironment {
        runtime_dir,
        socket,
    })
}

pub fn validate_socket_name(socket: &str) -> Result<(), String> {
    if socket.is_empty() || socket == "." || socket == ".." || socket.contains('/') {
        return Err("socket name must be a non-empty filename, not a path".to_owned());
    }
    Ok(())
}

fn validate_runtime_dir(runtime_dir: &Path) -> Result<(), String> {
    if !runtime_dir.is_absolute() {
        return Err("XDG_RUNTIME_DIR must be an absolute path".to_owned());
    }
    let metadata = fs::metadata(runtime_dir).map_err(|error| {
        format!(
            "cannot inspect XDG_RUNTIME_DIR {}: {error}",
            runtime_dir.display()
        )
    })?;
    if !metadata.is_dir() {
        return Err("XDG_RUNTIME_DIR must name a directory".to_owned());
    }
    let probe = runtime_dir.join(format!(".sol-session-probe-{}", std::process::id()));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map_err(|error| format!("XDG_RUNTIME_DIR is not writable: {error}"))?;
    fs::remove_file(&probe)
        .map_err(|error| format!("could not remove runtime directory probe: {error}"))?;
    Ok(())
}

pub fn run(plan: &LaunchPlan) -> Result<(), String> {
    let running = Arc::new(AtomicBool::new(true));
    let interrupted = Arc::clone(&running);
    ctrlc::set_handler(move || interrupted.store(false, Ordering::SeqCst))
        .map_err(|error| format!("cannot install shutdown handler: {error}"))?;

    let mut compositor = spawn(&plan.compositor)?;
    if let Err(error) = wait_for_socket(&mut compositor, &plan.socket_path, &running) {
        stop(&mut compositor);
        return Err(error);
    }
    let mut shell = match spawn(&plan.shell) {
        Ok(shell) => shell,
        Err(error) => {
            stop(&mut compositor);
            return Err(error);
        }
    };

    let result = supervise(&mut compositor, &mut shell, &running);
    stop(&mut compositor);
    stop(&mut shell);
    result
}

fn spawn(plan: &ProcessPlan) -> Result<Child, String> {
    let mut command = Command::new(&plan.program);
    command
        .args(&plan.arguments)
        .envs(plan.environment.iter().cloned());
    command
        .spawn()
        .map_err(|error| format!("failed to start {}: {error}", plan.program.display()))
}

fn wait_for_socket(
    compositor: &mut Child,
    socket: &Path,
    running: &AtomicBool,
) -> Result<(), String> {
    let deadline = Instant::now() + READY_TIMEOUT;
    while Instant::now() < deadline {
        if !running.load(Ordering::SeqCst) {
            return Err("session launch interrupted before compositor became ready".to_owned());
        }
        if let Some(status) = compositor
            .try_wait()
            .map_err(|error| format!("could not inspect compositor: {error}"))?
        {
            return Err(format!("compositor exited before readiness: {status}"));
        }
        if socket.exists() {
            return Ok(());
        }
        thread::sleep(POLL_INTERVAL);
    }
    Err(format!(
        "timed out waiting for compositor socket {}",
        socket.display()
    ))
}

fn supervise(
    compositor: &mut Child,
    shell: &mut Child,
    running: &AtomicBool,
) -> Result<(), String> {
    while running.load(Ordering::SeqCst) {
        if let Some(status) = compositor.try_wait().map_err(child_error("compositor"))? {
            return Err(format!(
                "compositor exited while shell was running: {status}"
            ));
        }
        if let Some(status) = shell.try_wait().map_err(child_error("shell"))? {
            return status_result("shell", status);
        }
        thread::sleep(POLL_INTERVAL);
    }
    Ok(())
}

fn child_error(name: &'static str) -> impl FnOnce(io::Error) -> String {
    move |error| format!("could not inspect {name}: {error}")
}

fn status_result(name: &str, status: ExitStatus) -> Result<(), String> {
    if status.success() {
        Ok(())
    } else {
        Err(format!("{name} exited unsuccessfully: {status}"))
    }
}

fn stop(child: &mut Child) {
    if child.try_wait().ok().flatten().is_none() {
        let _ = child.kill();
    }
    let _ = child.wait();
}

fn env_path(variable: &str, default: &str) -> PathBuf {
    env::var_os(variable).map_or_else(|| PathBuf::from(default), PathBuf::from)
}

fn value(value: &OsString) -> String {
    value.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_plan_has_the_required_backend_and_wayland_contract() {
        let environment = SessionEnvironment {
            runtime_dir: PathBuf::from("/run/user/1000"),
            socket: "wayland-sol-test".to_owned(),
        };
        let programs = ProgramPaths {
            compositor: PathBuf::from("/usr/bin/sol-compositor"),
            shell: PathBuf::from("/usr/bin/sol-shell"),
        };
        let plan = LaunchPlan::new(&environment, &programs);
        assert_eq!(plan.compositor.arguments, [OsString::from("--tty-udev")]);
        assert!(plan.compositor.environment.contains(&(
            OsString::from("SOL_WAYLAND_SOCKET"),
            OsString::from("wayland-sol-test")
        )));
        assert!(plan.shell.environment.contains(&(
            OsString::from("WAYLAND_DISPLAY"),
            OsString::from("wayland-sol-test")
        )));
        for process in [&plan.compositor, &plan.shell] {
            assert!(
                process
                    .environment
                    .contains(&(OsString::from("XDG_CURRENT_DESKTOP"), OsString::from("SOL")))
            );
            assert!(
                process
                    .environment
                    .contains(&(OsString::from("XDG_SESSION_DESKTOP"), OsString::from("SOL")))
            );
        }
        assert_eq!(
            plan.socket_path,
            PathBuf::from("/run/user/1000/wayland-sol-test")
        );
    }

    #[test]
    fn dry_run_is_deterministic() {
        let environment = SessionEnvironment {
            runtime_dir: PathBuf::from("/run/user/42"),
            socket: "wayland-sol".to_owned(),
        };
        let plan = LaunchPlan::new(
            &environment,
            &ProgramPaths {
                compositor: PathBuf::from("sol-compositor"),
                shell: PathBuf::from("sol-shell"),
            },
        );
        assert_eq!(
            plan.dry_run_output(),
            "compositor: sol-compositor --tty-udev\ncompositor env: XDG_RUNTIME_DIR=/run/user/42 SOL_WAYLAND_SOCKET=wayland-sol XDG_CURRENT_DESKTOP=SOL XDG_SESSION_DESKTOP=SOL\nshell: sol-shell\nshell env: XDG_RUNTIME_DIR=/run/user/42 WAYLAND_DISPLAY=wayland-sol XDG_CURRENT_DESKTOP=SOL XDG_SESSION_DESKTOP=SOL\nwait for socket: /run/user/42/wayland-sol\n"
        );
    }

    #[test]
    fn cli_accepts_dry_run_and_socket() {
        assert_eq!(
            parse_cli([
                "--dry-run".to_owned(),
                "--socket".to_owned(),
                "test".to_owned()
            ]),
            Ok(Cli {
                dry_run: true,
                socket_override: Some("test".to_owned())
            })
        );
        assert!(parse_cli(["--socket".to_owned()]).is_err());
    }

    #[test]
    fn socket_must_not_escape_the_runtime_directory() {
        for invalid in ["", ".", "..", "nested/socket"] {
            assert!(validate_socket_name(invalid).is_err(), "{invalid:?}");
        }
        assert!(validate_socket_name("wayland-sol-1").is_ok());
    }
}
