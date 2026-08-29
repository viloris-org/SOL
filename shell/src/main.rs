//! SOL desktop shell using the native SOL Compositor Protocol.

use sol_design::accessibility::TokenMode;
use sol_diagnostics::{DiagnosticSource, SolComponent, install_default_panic_capture};
use sol_scheduler::{SHELL_RT_PRIORITY, promote_current_thread};
use sol_shell::{
    catalog::bundled_app_catalog,
    desktop::{DesktopSession, SessionFlow},
    launcher::{ShellLauncher, UnavailableDesktopAdapter},
    scp_host::{HostOutput, ScpDesktopHost},
    system_status::SystemStatus,
};
use sol_system::{
    DefaultDenyPolicy, MemoryActionAuditStore, MemoryPermissionStore, SystemActionService,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::{io::Write, os::unix::net::UnixStream};
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
    let catalog = bundled_app_catalog();
    let launcher = ShellLauncher::new(
        SystemActionService::new(
            DefaultDenyPolicy,
            MemoryPermissionStore::default(),
            MemoryActionAuditStore::default(),
        ),
        UnavailableDesktopAdapter,
        catalog.clone(),
    );
    let mut status = SystemStatus::connect();
    let mut snapshot = status.snapshot();
    let host = ScpDesktopHost::connect()?;
    // SCP configures the first layer surface to the real output extent. The
    // bootstrap extent is used only to create that first background frame and
    // is replaced synchronously in `DesktopSession::start`.
    let mut desktop = DesktopSession::new(
        host,
        HostOutput::new(1920, 1080, 1.0),
        TokenMode::dark(),
        launcher,
        catalog,
        snapshot.clone(),
    );
    desktop.start()?;
    signal_desktop_ready();
    if once {
        tracing::info!("sol-shell desktop surfaces committed");
        return Ok(());
    }

    let running = Arc::new(AtomicBool::new(true));
    let signal_flag = Arc::clone(&running);
    ctrlc::set_handler(move || signal_flag.store(false, Ordering::Release))?;
    while running.load(Ordering::Acquire) {
        if desktop.pump()? == SessionFlow::Stop {
            break;
        }
        let next = status.snapshot();
        if next != snapshot {
            desktop.refresh_status(next.clone())?;
            snapshot = next;
        }
    }
    tracing::info!("sol-shell exiting cleanly");
    Ok(())
}

/// Notify the greeter only after the complete desktop has committed its first
/// background, Dock, and top-bar frames.
fn signal_desktop_ready() {
    let Some(path) = std::env::var_os("SOL_SESSION_READY_SOCKET") else {
        return;
    };
    match UnixStream::connect(&path) {
        Ok(mut stream) => {
            if let Err(error) = stream.write_all(b"ready\n") {
                tracing::warn!(%error, "could not finish the desktop-ready handshake");
            }
        }
        Err(error) => tracing::warn!(%error, "could not signal that the desktop is ready"),
    }
}
