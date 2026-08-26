//! Session-launch contract for an installed SOL desktop.
//!
//! This crate deliberately owns process ordering and environment propagation,
//! not seat/login management. The compositor remains responsible for opening a
//! real DRM/libseat session when its `--tty-udev` backend is selected.

use sol_scheduler::{ProcessClass, SchedulingManager};
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
const SERVICE_READY_GRACE: Duration = Duration::from_millis(100);
const POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionEnvironment {
    pub runtime_dir: PathBuf,
    pub socket: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramPaths {
    pub compositor: PathBuf,
    pub audio: PathBuf,
    pub shell: PathBuf,
    pub settingsd: PathBuf,
    pub notificationd: PathBuf,
    pub portal: PathBuf,
}

impl ProgramPaths {
    #[must_use]
    pub fn from_environment() -> Self {
        Self {
            compositor: env_path("SOL_COMPOSITOR_BIN", "sol-compositor"),
            audio: env_path("SOL_AUDIO_BIN", "pipewire"),
            shell: env_path("SOL_SHELL_BIN", "sol-shell"),
            settingsd: env_path("SOL_SETTINGSD_BIN", "sol-settingsd"),
            notificationd: env_path("SOL_NOTIFICATIOND_BIN", "sol-notificationd"),
            portal: env_path("SOL_PORTAL_BIN", "sol-portal"),
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
    pub audio: ProcessPlan,
    pub shell: ProcessPlan,
    pub settingsd: ProcessPlan,
    pub notificationd: ProcessPlan,
    pub portal: ProcessPlan,
    pub socket_path: PathBuf,
}

/// Session-level lifecycle used to preserve compositor-owned surface state
/// across a suspend/resume boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionPhase {
    Active,
    Suspending,
    Suspended,
    Resuming,
}

/// Renderer-neutral checkpoint for one live surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceCheckpoint {
    pub surface_id: u64,
    pub application: String,
    pub workspace: u32,
    pub geometry: (i32, i32, u32, u32),
}

/// Complete checkpoint captured before the display session sleeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionCheckpoint {
    pub generation: u64,
    pub surfaces: Vec<SurfaceCheckpoint>,
}

/// Durable boundary for a compositor/session restoration adapter.
pub trait SessionCheckpointStore {
    fn save(&mut self, checkpoint: &SessionCheckpoint) -> Result<(), String>;
    fn load(&self) -> Result<Option<SessionCheckpoint>, String>;
}

/// In-memory store used by headless tests and embedders.
#[derive(Debug, Default, Clone)]
pub struct MemoryCheckpointStore {
    checkpoint: Option<SessionCheckpoint>,
}

impl SessionCheckpointStore for MemoryCheckpointStore {
    fn save(&mut self, checkpoint: &SessionCheckpoint) -> Result<(), String> {
        self.checkpoint = Some(checkpoint.clone());
        Ok(())
    }

    fn load(&self) -> Result<Option<SessionCheckpoint>, String> {
        Ok(self.checkpoint.clone())
    }
}

/// State machine coordinating checkpoint capture and restoration.
pub struct SessionRestoreCoordinator<S> {
    store: S,
    phase: SessionPhase,
}

impl<S: SessionCheckpointStore> SessionRestoreCoordinator<S> {
    #[must_use]
    pub fn new(store: S) -> Self {
        Self {
            store,
            phase: SessionPhase::Active,
        }
    }

    #[must_use]
    pub const fn phase(&self) -> SessionPhase {
        self.phase
    }

    /// Capture validated surface state and persist it before suspend.
    pub fn suspend(&mut self, checkpoint: SessionCheckpoint) -> Result<(), SessionRestoreError> {
        if self.phase != SessionPhase::Active {
            return Err(SessionRestoreError::InvalidPhase(self.phase));
        }
        validate_checkpoint(&checkpoint)?;
        self.phase = SessionPhase::Suspending;
        if let Err(error) = self.store.save(&checkpoint) {
            self.phase = SessionPhase::Active;
            return Err(SessionRestoreError::Store(error));
        }
        self.phase = SessionPhase::Suspended;
        Ok(())
    }

    /// Reload the last checkpoint and return it to a compositor restore adapter.
    pub fn resume(&mut self) -> Result<SessionCheckpoint, SessionRestoreError> {
        if self.phase != SessionPhase::Suspended {
            return Err(SessionRestoreError::InvalidPhase(self.phase));
        }
        self.phase = SessionPhase::Resuming;
        let checkpoint = match self.store.load() {
            Ok(Some(checkpoint)) => checkpoint,
            Ok(None) => {
                self.phase = SessionPhase::Suspended;
                return Err(SessionRestoreError::MissingCheckpoint);
            }
            Err(error) => {
                self.phase = SessionPhase::Suspended;
                return Err(SessionRestoreError::Store(error));
            }
        };
        if let Err(error) = validate_checkpoint(&checkpoint) {
            self.phase = SessionPhase::Suspended;
            return Err(error);
        }
        self.phase = SessionPhase::Active;
        Ok(checkpoint)
    }
}

/// Validation or persistence failure in the restoration boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionRestoreError {
    InvalidPhase(SessionPhase),
    InvalidCheckpoint(&'static str),
    DuplicateSurface(u64),
    MissingCheckpoint,
    Store(String),
}

fn validate_checkpoint(checkpoint: &SessionCheckpoint) -> Result<(), SessionRestoreError> {
    if checkpoint.generation == 0 {
        return Err(SessionRestoreError::InvalidCheckpoint(
            "checkpoint generation must be non-zero",
        ));
    }
    let mut ids = std::collections::BTreeSet::new();
    for surface in &checkpoint.surfaces {
        if surface.surface_id == 0 {
            return Err(SessionRestoreError::InvalidCheckpoint(
                "surface ID must be non-zero",
            ));
        }
        if !ids.insert(surface.surface_id) {
            return Err(SessionRestoreError::DuplicateSurface(surface.surface_id));
        }
        if surface.application.is_empty() || surface.application.len() > 256 {
            return Err(SessionRestoreError::InvalidCheckpoint(
                "surface application identity is invalid",
            ));
        }
        if surface.geometry.2 == 0 || surface.geometry.3 == 0 {
            return Err(SessionRestoreError::InvalidCheckpoint(
                "surface geometry must have non-zero size",
            ));
        }
    }
    Ok(())
}

impl LaunchPlan {
    #[must_use]
    pub fn new(environment: &SessionEnvironment, programs: &ProgramPaths) -> Self {
        let runtime_dir = environment.runtime_dir.as_os_str().to_os_string();
        let socket = OsString::from(&environment.socket);
        let desktop_environment = vec![
            (OsString::from("XDG_RUNTIME_DIR"), runtime_dir.clone()),
            (OsString::from("XDG_CURRENT_DESKTOP"), OsString::from("SOL")),
            (OsString::from("XDG_SESSION_DESKTOP"), OsString::from("SOL")),
        ];
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
            audio: ProcessPlan {
                program: programs.audio.clone(),
                arguments: Vec::new(),
                environment: desktop_environment.clone(),
            },
            shell: ProcessPlan {
                program: programs.shell.clone(),
                arguments: Vec::new(),
                environment: vec![
                    (OsString::from("XDG_RUNTIME_DIR"), runtime_dir.clone()),
                    (OsString::from("WAYLAND_DISPLAY"), socket),
                    (OsString::from("XDG_CURRENT_DESKTOP"), OsString::from("SOL")),
                    (OsString::from("XDG_SESSION_DESKTOP"), OsString::from("SOL")),
                ],
            },
            settingsd: service_plan(&programs.settingsd, &desktop_environment),
            notificationd: service_plan(&programs.notificationd, &desktop_environment),
            portal: service_plan(&programs.portal, &desktop_environment),
            socket_path: environment.runtime_dir.join(&environment.socket),
        }
    }

    #[must_use]
    pub fn dry_run_output(&self) -> String {
        format!(
            "compositor: {} --tty-udev\ncompositor env: XDG_RUNTIME_DIR={} SOL_WAYLAND_SOCKET={} XDG_CURRENT_DESKTOP={} XDG_SESSION_DESKTOP={}\naudio: {}\nsettingsd: {} --dbus\nnotificationd: {} --dbus\nportal: {} --dbus\nshell: {}\nshell env: XDG_RUNTIME_DIR={} WAYLAND_DISPLAY={} XDG_CURRENT_DESKTOP={} XDG_SESSION_DESKTOP={}\nwait for socket: {}\n",
            self.compositor.program.display(),
            value(&self.compositor.environment[0].1),
            value(&self.compositor.environment[1].1),
            value(&self.compositor.environment[2].1),
            value(&self.compositor.environment[3].1),
            self.audio.program.display(),
            self.settingsd.program.display(),
            self.notificationd.program.display(),
            self.portal.program.display(),
            self.shell.program.display(),
            value(&self.shell.environment[0].1),
            value(&self.shell.environment[1].1),
            value(&self.shell.environment[2].1),
            value(&self.shell.environment[3].1),
            self.socket_path.display(),
        )
    }

    fn services(&self) -> [(&'static str, &ProcessPlan, ProcessClass); 4] {
        [
            ("audio", &self.audio, ProcessClass::Audio),
            ("settingsd", &self.settingsd, ProcessClass::System),
            (
                "notificationd",
                &self.notificationd,
                ProcessClass::Notification,
            ),
            ("portal", &self.portal, ProcessClass::System),
        ]
    }
}

fn service_plan(program: &Path, environment: &[(OsString, OsString)]) -> ProcessPlan {
    ProcessPlan {
        program: program.to_path_buf(),
        arguments: vec![OsString::from("--dbus")],
        environment: environment.to_vec(),
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

    let cgroup_root = env::var_os("SOL_CGROUP_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/sys/fs/cgroup/sol"));
    let mut scheduling = SchedulingManager::new(cgroup_root);
    let build_containment_enabled = scheduling
        .provision()
        .map_err(|error| {
            eprintln!("sol-session: cgroup hierarchy unavailable: {error}");
            error
        })
        .is_ok();

    let mut compositor = spawn(&plan.compositor, &scheduling, ProcessClass::Compositor)?;
    if let Err(error) = wait_for_socket(&mut compositor, &plan.socket_path, &running) {
        stop(&mut compositor);
        return Err(error);
    }
    let mut companions = Vec::new();
    for (name, child_plan, class) in plan.services() {
        match spawn(child_plan, &scheduling, class) {
            Ok(child) => companions.push(ManagedChild {
                name,
                plan: child_plan,
                class,
                child,
            }),
            Err(error) => {
                stop(&mut compositor);
                stop_all(&mut companions);
                return Err(error);
            }
        }
    }
    if let Err(error) = wait_for_services(&mut compositor, &mut companions, &running) {
        stop(&mut compositor);
        stop_all(&mut companions);
        return Err(error);
    }
    match spawn(&plan.shell, &scheduling, ProcessClass::Shell) {
        Ok(child) => companions.push(ManagedChild {
            name: "shell",
            plan: &plan.shell,
            class: ProcessClass::Shell,
            child,
        }),
        Err(error) => {
            stop(&mut compositor);
            stop_all(&mut companions);
            return Err(error);
        }
    }

    let result = supervise(
        &mut compositor,
        &mut companions,
        &running,
        &mut scheduling,
        build_containment_enabled,
    );
    stop(&mut compositor);
    stop_all(&mut companions);
    result
}

struct ManagedChild<'a> {
    name: &'static str,
    plan: &'a ProcessPlan,
    class: ProcessClass,
    child: Child,
}

fn spawn(
    plan: &ProcessPlan,
    scheduling: &SchedulingManager,
    class: ProcessClass,
) -> Result<Child, String> {
    let mut command = Command::new(&plan.program);
    command
        .args(&plan.arguments)
        .envs(plan.environment.iter().cloned());
    let child = command
        .spawn()
        .map_err(|error| format!("failed to start {}: {error}", plan.program.display()))?;
    let pid = child.id();
    for failure in scheduling.apply(pid, class).failures {
        eprintln!(
            "sol-session: {class:?} PID {pid} {} unavailable: {}",
            failure.control, failure.error
        );
    }
    Ok(child)
}

fn wait_for_services(
    compositor: &mut Child,
    services: &mut [ManagedChild<'_>],
    running: &AtomicBool,
) -> Result<(), String> {
    let deadline = Instant::now() + SERVICE_READY_GRACE;
    while Instant::now() < deadline {
        if !running.load(Ordering::SeqCst) {
            return Err("session launch interrupted while services started".to_owned());
        }
        if let Some(status) = compositor.try_wait().map_err(child_error("compositor"))? {
            return Err(format!(
                "compositor exited while services started: {status}"
            ));
        }
        for service in &mut *services {
            if let Some(status) = service
                .child
                .try_wait()
                .map_err(child_error(service.name))?
            {
                return Err(format!(
                    "{} exited during service startup: {status}",
                    service.name
                ));
            }
        }
        thread::sleep(POLL_INTERVAL);
    }
    Ok(())
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
    companions: &mut [ManagedChild<'_>],
    running: &AtomicBool,
    scheduling: &mut SchedulingManager,
    build_containment_enabled: bool,
) -> Result<(), String> {
    let mut next_build_scan = Instant::now();
    while running.load(Ordering::SeqCst) {
        if let Some(status) = compositor.try_wait().map_err(child_error("compositor"))? {
            return Err(format!(
                "compositor exited while shell was running: {status}"
            ));
        }
        for companion in &mut *companions {
            if let Some(status) = companion
                .child
                .try_wait()
                .map_err(child_error(companion.name))?
            {
                tracing_restart(companion.name, status);
                companion.child =
                    spawn(companion.plan, scheduling, companion.class).map_err(|error| {
                        format!(
                            "could not restart {} after {status}: {error}",
                            companion.name
                        )
                    })?;
            }
        }
        if build_containment_enabled && Instant::now() >= next_build_scan {
            if let Err(error) = scheduling.contain_build_processes() {
                eprintln!("sol-session: build-process scan failed: {error}");
            }
            next_build_scan = Instant::now() + Duration::from_millis(500);
        }
        thread::sleep(POLL_INTERVAL);
    }
    Ok(())
}

fn child_error(name: &'static str) -> impl FnOnce(io::Error) -> String {
    move |error| format!("could not inspect {name}: {error}")
}

fn tracing_restart(name: &str, status: ExitStatus) {
    eprintln!("sol-session: restarting {name} after exit: {status}");
}

fn stop(child: &mut Child) {
    if child.try_wait().ok().flatten().is_none() {
        let _ = child.kill();
    }
    let _ = child.wait();
}

fn stop_all(children: &mut [ManagedChild<'_>]) {
    for child in children {
        stop(&mut child.child);
    }
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
            audio: PathBuf::from("/usr/bin/pipewire"),
            shell: PathBuf::from("/usr/bin/sol-shell"),
            settingsd: PathBuf::from("/usr/bin/sol-settingsd"),
            notificationd: PathBuf::from("/usr/bin/sol-notificationd"),
            portal: PathBuf::from("/usr/bin/sol-portal"),
        };
        let plan = LaunchPlan::new(&environment, &programs);
        assert_eq!(plan.compositor.arguments, [OsString::from("--tty-udev")]);
        assert!(plan.audio.arguments.is_empty());
        for (_, service) in [
            ("settingsd", &plan.settingsd),
            ("notificationd", &plan.notificationd),
            ("portal", &plan.portal),
        ] {
            assert_eq!(service.arguments, [OsString::from("--dbus")]);
        }
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
                audio: PathBuf::from("pipewire"),
                shell: PathBuf::from("sol-shell"),
                settingsd: PathBuf::from("sol-settingsd"),
                notificationd: PathBuf::from("sol-notificationd"),
                portal: PathBuf::from("sol-portal"),
            },
        );
        assert_eq!(
            plan.dry_run_output(),
            "compositor: sol-compositor --tty-udev\ncompositor env: XDG_RUNTIME_DIR=/run/user/42 SOL_WAYLAND_SOCKET=wayland-sol XDG_CURRENT_DESKTOP=SOL XDG_SESSION_DESKTOP=SOL\naudio: pipewire\nsettingsd: sol-settingsd --dbus\nnotificationd: sol-notificationd --dbus\nportal: sol-portal --dbus\nshell: sol-shell\nshell env: XDG_RUNTIME_DIR=/run/user/42 WAYLAND_DISPLAY=wayland-sol XDG_CURRENT_DESKTOP=SOL XDG_SESSION_DESKTOP=SOL\nwait for socket: /run/user/42/wayland-sol\n"
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

    #[test]
    fn suspend_persists_and_resume_restores_surface_checkpoint() {
        let mut coordinator = SessionRestoreCoordinator::new(MemoryCheckpointStore::default());
        let checkpoint = SessionCheckpoint {
            generation: 7,
            surfaces: vec![SurfaceCheckpoint {
                surface_id: 11,
                application: "com.example.notes".to_owned(),
                workspace: 2,
                geometry: (40, 50, 800, 600),
            }],
        };
        coordinator.suspend(checkpoint.clone()).unwrap();
        assert_eq!(coordinator.phase(), SessionPhase::Suspended);
        assert_eq!(coordinator.resume().unwrap(), checkpoint);
        assert_eq!(coordinator.phase(), SessionPhase::Active);
    }

    #[test]
    fn invalid_or_out_of_order_restore_operations_are_rejected() {
        let mut coordinator = SessionRestoreCoordinator::new(MemoryCheckpointStore::default());
        assert!(matches!(
            coordinator.resume(),
            Err(SessionRestoreError::InvalidPhase(SessionPhase::Active))
        ));
        let invalid = SessionCheckpoint {
            generation: 0,
            surfaces: Vec::new(),
        };
        assert!(matches!(
            coordinator.suspend(invalid),
            Err(SessionRestoreError::InvalidCheckpoint(_))
        ));
        let duplicate = SessionCheckpoint {
            generation: 1,
            surfaces: vec![
                SurfaceCheckpoint {
                    surface_id: 1,
                    application: "com.example.one".to_owned(),
                    workspace: 0,
                    geometry: (0, 0, 1, 1),
                },
                SurfaceCheckpoint {
                    surface_id: 1,
                    application: "com.example.two".to_owned(),
                    workspace: 0,
                    geometry: (0, 0, 1, 1),
                },
            ],
        };
        assert_eq!(
            coordinator.suspend(duplicate),
            Err(SessionRestoreError::DuplicateSurface(1))
        );
    }
}
