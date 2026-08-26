use anyhow::{Context, Result};
use nix::sys::signal::{self, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use std::collections::HashMap;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, ExitStatus};
use tracing::{error, info, warn};

use crate::daemon::{DaemonDefinition, RestartPolicy};

#[derive(Debug)]
pub struct RunningDaemon {
    pub pid: u32,
    pub restart_count: u32,
    pub definition: DaemonDefinition,
}

pub struct ProcessManager {
    running: HashMap<String, RunningDaemon>,
    max_restart_count: u32,
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            running: HashMap::new(),
            max_restart_count: 5,
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
}
