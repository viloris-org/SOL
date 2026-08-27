//! Native SCP compositor service.
//!
//! The process deliberately exposes only the SOL Compositor Protocol.  The
//! retired Smithay/Wayland implementation remains in the repository history
//! and migration notes, but is no longer part of the build graph.

use sol_compositor::scp::ScpServer;
use std::{
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    } else {
        tracing_subscriber::fmt().init();
    }

    let server = ScpServer::bind_from_env()?;
    spawn_client(spawn_argument(), server.socket_path());

    let running = Arc::new(AtomicBool::new(true));
    let signal_flag = Arc::clone(&running);
    ctrlc::set_handler(move || signal_flag.store(false, Ordering::Release))?;

    tracing::info!(socket = %server.socket_path().display(), "native SCP compositor ready");
    while running.load(Ordering::Acquire) {
        thread::sleep(Duration::from_millis(50));
    }
    tracing::info!("native SCP compositor exiting");
    Ok(())
}

fn spawn_argument() -> Option<String> {
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        if argument == "--spawn" {
            return arguments.next();
        }
    }
    None
}

fn spawn_client(client: Option<String>, socket_path: &std::path::Path) {
    let Some(client) = client else {
        return;
    };

    match Command::new(&client)
        .env("SOL_SCP_SOCKET", socket_path)
        .spawn()
    {
        Ok(child) => tracing::info!(pid = child.id(), %client, "spawned SCP client"),
        Err(error) => tracing::error!(%error, %client, "failed to spawn SCP client"),
    }
}
