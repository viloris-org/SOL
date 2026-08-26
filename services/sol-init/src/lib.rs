pub mod daemon;
pub mod process;

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tracing::{error, info};

use daemon::{DaemonDefinition, StartMode};
use process::ProcessManager;

pub struct SolInit {
    daemons: HashMap<String, DaemonDefinition>,
    process_manager: ProcessManager,
    system_daemon_dir: PathBuf,
    user_daemon_dir: Option<PathBuf>,
}

impl SolInit {
    pub fn new(system_daemon_dir: PathBuf, user_daemon_dir: Option<PathBuf>) -> Self {
        Self {
            daemons: HashMap::new(),
            process_manager: ProcessManager::new(),
            system_daemon_dir,
            user_daemon_dir,
        }
    }

    pub fn load_daemons(&mut self) -> Result<()> {
        info!("Loading system daemons from {:?}", self.system_daemon_dir);
        let system_daemons = DaemonDefinition::load_from_dir(&self.system_daemon_dir)?;
        info!("Loaded {} system daemons", system_daemons.len());

        self.daemons.extend(system_daemons);

        // Phase 2+: Load user/application daemons
        if let Some(ref user_dir) = self.user_daemon_dir {
            if user_dir.exists() {
                info!("Loading user daemons from {:?}", user_dir);
                let user_daemons = DaemonDefinition::load_from_dir(user_dir)?;
                info!("Loaded {} user daemons", user_daemons.len());
                self.daemons.extend(user_daemons);
            }
        }

        Ok(())
    }

    pub fn start_boot_daemons(&mut self) -> Result<()> {
        // Get daemons that should start at boot
        let boot_daemons: HashMap<String, DaemonDefinition> = self
            .daemons
            .iter()
            .filter(|(_, d)| d.daemon.start_mode == StartMode::Boot)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        info!("Starting {} boot daemons", boot_daemons.len());

        // Topologically sort by dependencies
        let order = daemon::topological_sort(&boot_daemons)
            .context("Failed to resolve daemon dependencies")?;

        // Start daemons in order
        for name in order {
            if let Some(daemon) = self.daemons.get(&name) {
                self.process_manager.start_daemon(&name, daemon)?;
            }
        }

        Ok(())
    }

    pub fn activate_daemon(&mut self, name: &str) -> Result<()> {
        // Check if already running
        if self.process_manager.is_running(name) {
            info!("Daemon {} is already running", name);
            return Ok(());
        }

        // Find daemon definition
        let daemon = self
            .daemons
            .get(name)
            .with_context(|| format!("Daemon {} not found", name))?;

        info!("Activating daemon: {}", name);
        self.process_manager.start_daemon(name, daemon)?;

        Ok(())
    }

    pub fn run(&mut self) -> Result<()> {
        info!("sol-init main loop started");

        loop {
            // Poll for exited processes (non-blocking)
            match self.process_manager.wait_any() {
                Ok(Some((name, status))) => {
                    if let Err(e) = self.process_manager.handle_exit(&name, status) {
                        error!("Error handling exit of {}: {}", name, e);
                    }
                }
                Ok(None) => {
                    // No process exited, sleep a bit
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    error!("Error waiting for processes: {}", e);
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
        }
    }

    pub fn shutdown(&mut self) -> Result<()> {
        info!("Shutting down sol-init");
        self.process_manager.stop_all()?;
        Ok(())
    }
}
