use anyhow::{Context, Result};
use nix::sys::signal::{self, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use sol_scheduler::{ProcessClass, SchedulingManager};
use std::collections::HashMap;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::{Command, ExitStatus};
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

use crate::daemon::{DaemonDefinition, DaemonType, RestartPolicy};

const BUILD_SCAN_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug)]
pub struct RunningDaemon {
    pub pid: u32,
    pub restart_count: u32,
    pub definition: DaemonDefinition,
}

pub struct ProcessManager {
    running: HashMap<String, RunningDaemon>,
    max_restart_count: u32,
    scheduling: SchedulingManager,
    last_build_scan: Instant,
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessManager {
    pub fn new() -> Self {
        let cgroup_root = std::env::var_os("SOL_CGROUP_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/sys/fs/cgroup/sol"));
        Self::with_scheduling_manager(SchedulingManager::new(cgroup_root))
    }

    /// Construct a manager with an injected scheduler for tests and alternate
    /// delegated cgroup roots.
    pub fn with_scheduling_manager(scheduling: SchedulingManager) -> Self {
        if let Err(error) = scheduling.provision() {
            warn!(%error, "SOL cgroup hierarchy unavailable; process controls will degrade independently");
        }
        Self {
            running: HashMap::new(),
            max_restart_count: 5,
            scheduling,
            last_build_scan: Instant::now() - BUILD_SCAN_INTERVAL,
        }
    }

    pub fn start_daemon(&mut self, name: &str, daemon: &DaemonDefinition) -> Result<()> {
        info!("Starting daemon: {}", name);

        let mut cmd = Command::new(&daemon.daemon.exec);

        // Set environment variables
        for (key, value) in &daemon.environment {
            cmd.env(key, value);
        }

        // Spawn the process
        let child = cmd
            .spawn()
            .with_context(|| format!("Failed to spawn daemon: {}", name))?;

        let pid = child.id();
        info!("Daemon {} started with PID {}", name, pid);
        self.apply_scheduling(pid, daemon);

        self.running.insert(
            name.to_string(),
            RunningDaemon {
                pid,
                restart_count: 0,
                definition: daemon.clone(),
            },
        );

        Ok(())
    }

    pub fn is_running(&self, name: &str) -> bool {
        self.running.contains_key(name)
    }

    pub fn wait_any(&mut self) -> Result<Option<(String, ExitStatus)>> {
        // Check all running daemons
        for (name, daemon) in &self.running {
            let pid = Pid::from_raw(daemon.pid as i32);

            match waitpid(pid, Some(WaitPidFlag::WNOHANG)) {
                Ok(WaitStatus::Exited(_, code)) => {
                    let name = name.clone();
                    let status = ExitStatus::from_raw(code << 8);
                    return Ok(Some((name, status)));
                }
                Ok(WaitStatus::Signaled(_, signal, _)) => {
                    let name = name.clone();
                    let status = ExitStatus::from_raw(signal as i32);
                    return Ok(Some((name, status)));
                }
                Ok(WaitStatus::StillAlive) => {
                    // This daemon is still running, check next
                    continue;
                }
                Ok(_) => {
                    // Other status (stopped, continued, etc.)
                    continue;
                }
                Err(e) => {
                    warn!("waitpid error for {}: {}", name, e);
                    continue;
                }
            }
        }

        Ok(None)
    }

    pub fn handle_exit(&mut self, name: &str, status: ExitStatus) -> Result<()> {
        let daemon = match self.running.remove(name) {
            Some(d) => d,
            None => {
                warn!("Daemon {} not found in running list", name);
                return Ok(());
            }
        };

        if status.success() {
            info!("Daemon {} exited successfully", name);
        } else {
            error!("Daemon {} exited with status: {:?}", name, status);
        }

        let should_restart = match daemon.definition.daemon.restart_policy {
            RestartPolicy::Always => true,
            RestartPolicy::OnFailure => !status.success(),
            RestartPolicy::Never => false,
        };

        if should_restart {
            if daemon.restart_count >= self.max_restart_count {
                error!(
                    "Daemon {} has restarted {} times, giving up",
                    name, daemon.restart_count
                );
                return Ok(());
            }

            info!(
                "Restarting daemon {} (restart count: {})",
                name,
                daemon.restart_count + 1
            );

            // Wait a bit before restarting
            std::thread::sleep(std::time::Duration::from_secs(1));

            self.start_daemon_with_count(name, &daemon.definition, daemon.restart_count + 1)?;
        }

        Ok(())
    }

    fn start_daemon_with_count(
        &mut self,
        name: &str,
        daemon: &DaemonDefinition,
        restart_count: u32,
    ) -> Result<()> {
        info!("Starting daemon: {}", name);

        let mut cmd = Command::new(&daemon.daemon.exec);

        for (key, value) in &daemon.environment {
            cmd.env(key, value);
        }

        let child = cmd
            .spawn()
            .with_context(|| format!("Failed to spawn daemon: {}", name))?;

        let pid = child.id();
        info!(
            "Daemon {} started with PID {} (restart {})",
            name, pid, restart_count
        );
        self.apply_scheduling(pid, daemon);

        self.running.insert(
            name.to_string(),
            RunningDaemon {
                pid,
                restart_count,
                definition: daemon.clone(),
            },
        );

        Ok(())
    }

    pub fn stop_daemon(&mut self, name: &str) -> Result<()> {
        if let Some(daemon) = self.running.get(name) {
            let pid = Pid::from_raw(daemon.pid as i32);
            info!("Stopping daemon {} (PID {})", name, pid);

            signal::kill(pid, Signal::SIGTERM)
                .with_context(|| format!("Failed to send SIGTERM to daemon {}", name))?;

            self.running.remove(name);
        }

        Ok(())
    }

    pub fn stop_all(&mut self) -> Result<()> {
        let names: Vec<String> = self.running.keys().cloned().collect();

        for name in names {
            self.stop_daemon(&name)?;
        }

        Ok(())
    }

    pub fn running_daemons(&self) -> Vec<String> {
        self.running.keys().cloned().collect()
    }

    /// Move an application between trusted foreground/background classes.
    /// The class comes from shell/portal policy, not application metadata.
    pub fn set_process_class(&self, pid: u32, class: ProcessClass) {
        self.log_apply_report(pid, class, self.scheduling.apply(pid, class));
    }

    /// Periodically discover compiler processes and contain them in sol-build.
    pub fn maintain_scheduling(&mut self) {
        if self.last_build_scan.elapsed() < BUILD_SCAN_INTERVAL {
            return;
        }
        self.last_build_scan = Instant::now();
        match self.scheduling.contain_build_processes() {
            Ok(containment) => {
                for pid in containment.moved {
                    info!(pid, "moved build process into sol-build cgroup");
                }
                for (pid, error) in containment.failures {
                    warn!(pid, %error, "failed to contain build process");
                }
            }
            Err(error) => warn!(%error, "build-process scan failed"),
        }
    }

    fn apply_scheduling(&self, pid: u32, daemon: &DaemonDefinition) {
        let class = process_class_for(daemon);
        self.log_apply_report(pid, class, self.scheduling.apply(pid, class));
    }

    fn log_apply_report(&self, pid: u32, class: ProcessClass, report: sol_scheduler::ApplyReport) {
        for failure in report.failures {
            warn!(
                pid,
                ?class,
                control = failure.control,
                error = %failure.error,
                "scheduling control unavailable"
            );
        }
    }
}

fn process_class_for(daemon: &DaemonDefinition) -> ProcessClass {
    match daemon.daemon.name.as_str() {
        "sol-compositor" => ProcessClass::Compositor,
        "sol-audio" | "pipewire" => ProcessClass::Audio,
        "sol-shell" => ProcessClass::Shell,
        "sol-networkd" | "systemd-resolved" => ProcessClass::Network,
        "sol-notificationd" => ProcessClass::Notification,
        _ => match daemon.daemon.daemon_type {
            DaemonType::Application => ProcessClass::Background,
            DaemonType::Core | DaemonType::System => ProcessClass::System,
        },
    }
}

#[cfg(test)]
mod scheduling_tests {
    use super::*;
    use crate::daemon::{DaemonConfig, ResourceConfig, StartMode};

    fn daemon(name: &str, daemon_type: DaemonType) -> DaemonDefinition {
        DaemonDefinition {
            daemon: DaemonConfig {
                name: name.to_owned(),
                exec: "/bin/true".to_owned(),
                daemon_type,
                start_mode: StartMode::Boot,
                restart_policy: RestartPolicy::Never,
                after: Vec::new(),
                requires: Vec::new(),
                capabilities: Vec::new(),
                dbus_name: None,
            },
            environment: HashMap::new(),
            resources: ResourceConfig::default(),
        }
    }

    #[test]
    fn trusted_daemons_map_to_fixed_scheduler_classes() {
        assert_eq!(
            process_class_for(&daemon("sol-compositor", DaemonType::Core)),
            ProcessClass::Compositor
        );
        assert_eq!(
            process_class_for(&daemon("sol-networkd", DaemonType::System)),
            ProcessClass::Network
        );
        assert_eq!(
            process_class_for(&daemon("third-party", DaemonType::Application)),
            ProcessClass::Background
        );
    }
}
