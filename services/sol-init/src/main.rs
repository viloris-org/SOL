use anyhow::Result;
use sol_init::SolInit;
use std::path::PathBuf;
use tracing::{error, info};

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("SOL Init starting");

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();

    // Handle --activate <daemon-name> (for D-Bus activation)
    if args.len() == 3 && args[1] == "--activate" {
        let daemon_name = &args[2];
        info!("D-Bus activation requested for: {}", daemon_name);

        // For D-Bus activation, we need to connect to the running sol-init instance
        // For now, just start the daemon directly (Phase 1 simplification)
        let system_daemon_dir = PathBuf::from("/usr/share/sol/daemons");
        let mut sol_init = SolInit::new(system_daemon_dir, None);

        sol_init.load_daemons()?;
        sol_init.activate_daemon(daemon_name)?;

        return Ok(());
    }

    // Normal startup
    let system_daemon_dir = PathBuf::from("/usr/share/sol/daemons");

    // Phase 2+: Enable user daemon directory
    // let user_daemon_dir = dirs::home_dir().map(|p| p.join(".local/share/sol/daemons"));
    let user_daemon_dir = None;

    let mut sol_init = SolInit::new(system_daemon_dir, user_daemon_dir);

    // Load all daemon definitions
    sol_init.load_daemons()?;

    // Start boot daemons
    sol_init.start_boot_daemons()?;

    info!("All boot daemons started, entering main loop");

    // Setup signal handlers for graceful shutdown
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        info!("Received shutdown signal");
        r.store(false, std::sync::atomic::Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    // Main loop
    while running.load(std::sync::atomic::Ordering::SeqCst) {
        if let Err(e) = sol_init.run() {
            error!("Error in main loop: {}", e);
            break;
        }
    }

    // Shutdown
    sol_init.shutdown()?;
    info!("SOL Init exited");

    Ok(())
}
