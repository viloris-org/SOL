//! SOL desktop shell using the native SOL Compositor Protocol.

mod client;

use sol_diagnostics::{DiagnosticSource, SolComponent, install_default_panic_capture};
use sol_scheduler::{SHELL_RT_PRIORITY, promote_current_thread};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tracing::level_filters::LevelFilter;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::DEBUG)
        .init();

    if let Err(error) =
        install_default_panic_capture(DiagnosticSource::Component(SolComponent::Shell))
    {
        tracing::warn!(%error, "shell crash capture is unavailable");
    }
    if let Err(error) = promote_current_thread(SHELL_RT_PRIORITY) {
        tracing::warn!(%error, "SCHED_FIFO priority 1 unavailable; shell UI loop remains on CFS");
    }

    let once = std::env::args().any(|argument| argument == "--once");
    let client = client::ShellClient::connect()?;
    if once {
        tracing::info!("sol-shell SCP round-trip OK");
        return Ok(());
    }

    let running = Arc::new(AtomicBool::new(true));
    let signal_flag = Arc::clone(&running);
    ctrlc::set_handler(move || signal_flag.store(false, Ordering::Release))?;
    client.run(&running)?;
    tracing::info!("sol-shell exiting cleanly");
    Ok(())
}
