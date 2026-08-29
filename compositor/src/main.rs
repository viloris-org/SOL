//! Native SCP compositor service.
//!
//! The process deliberately exposes only the SOL Compositor Protocol.  The
//! retired compositor implementation remains in repository history and
//! migration notes, but is no longer present in the active source tree.
//!
//! This binary is the compositor's **headless backend**: it owns the output
//! topology and the presentation cadence, and drives
//! [`ScpState::present_frame`] to compose what clients have committed. It has no
//! display of its own — a DRM/KMS backend that scans the composed image out to
//! real hardware is separate, still-pending work — so the composed desktop is
//! observed through `SOL_SCP_FRAME_DUMP` or by composing directly from a test.

use sol_compositor::scp::{ScpServer, ScpState, protocol::OutputId};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

/// Presentation cadence of the headless backend.
///
/// Real hardware paces frames from the display's vblank. With no display, the
/// backend has to choose, and 60 Hz is what the rest of the desktop's motion
/// tokens assume.
const FRAME_INTERVAL: Duration = Duration::from_micros(16_666);

/// Output geometry the headless backend advertises when none is configured.
const DEFAULT_OUTPUT: (i32, i32, i32) = (1920, 1080, 60_000);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    } else {
        tracing_subscriber::fmt().init();
    }

    let server = ScpServer::bind_from_env()?;
    let state = server.state();
    let output_id = register_output(&state)?;
    spawn_client(spawn_argument(), server.socket_path());

    let running = Arc::new(AtomicBool::new(true));
    let signal_flag = Arc::clone(&running);
    ctrlc::set_handler(move || signal_flag.store(false, Ordering::Release))?;

    tracing::info!(socket = %server.socket_path().display(), "native SCP compositor ready");
    present_loop(&state, output_id, frame_dump_path(), &running);
    tracing::info!("native SCP compositor exiting");
    Ok(())
}

/// Register the backend's output.
///
/// Outputs describe hardware, so [`ScpState`] does not invent one: it is the
/// backend that knows what displays exist. The headless backend has exactly one
/// virtual display, sized from `SOL_SCP_OUTPUT` (`WIDTHxHEIGHT`) so a test or a
/// developer can compose a desktop at a size other than 1080p.
fn register_output(state: &Arc<Mutex<ScpState>>) -> Result<OutputId, Box<dyn std::error::Error>> {
    let (width, height, refresh) = configured_output();
    let mut guard = state
        .lock()
        .map_err(|_| "compositor state lock was poisoned before the first frame")?;
    let output_id = guard.output_manager_mut().add_output(
        "HEADLESS-1".to_string(),
        "SOL headless output".to_string(),
        width,
        height,
        refresh,
    );
    tracing::info!(output_id, width, height, "headless output registered");
    Ok(output_id)
}

/// Compose and present until the process is asked to stop.
///
/// A frame is composed on every tick rather than only when a client commits.
/// That is more work than a damage-tracking compositor needs to do, and it is
/// deliberate for now: presentation correctness — buffers released and frame
/// callbacks fired in the right order — is what this loop is here to establish,
/// and a fixed cadence makes that behavior the same on every run.
fn present_loop(
    state: &Arc<Mutex<ScpState>>,
    output_id: OutputId,
    dump_path: Option<PathBuf>,
    running: &AtomicBool,
) {
    let start = Instant::now();
    let mut last_dumped: Option<Vec<u8>> = None;

    while running.load(Ordering::Acquire) {
        let frame_start = Instant::now();
        let timestamp_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);

        let framebuffer = match state.lock() {
            Ok(mut guard) => guard.present_frame(output_id, timestamp_ms),
            Err(_) => {
                tracing::error!("compositor state lock is poisoned; stopping presentation");
                return;
            }
        };

        if let (Some(path), Some(framebuffer)) = (dump_path.as_deref(), framebuffer.as_ref()) {
            // Only rewrite the dump when the desktop actually changed. Without
            // this the file is rewritten sixty times a second whether or not
            // anything moved, and anyone watching it sees churn instead of
            // change.
            if last_dumped.as_deref() != Some(framebuffer.pixels()) {
                write_frame_dump(path, &framebuffer.to_png());
                last_dumped = Some(framebuffer.pixels().to_vec());
            }
        }

        if let Some(remaining) = FRAME_INTERVAL.checked_sub(frame_start.elapsed()) {
            thread::sleep(remaining);
        }
    }
}

/// Write a composed frame to disk, replacing it atomically.
///
/// The dump is meant to be looked at while the compositor runs, so a reader must
/// never catch a half-written PNG: the frame is written beside the target and
/// renamed over it.
fn write_frame_dump(path: &Path, png: &[u8]) {
    let temporary = path.with_extension("png.partial");
    let write = std::fs::File::create(&temporary)
        .and_then(|mut file| file.write_all(png).and_then(|()| file.sync_all()))
        .and_then(|()| std::fs::rename(&temporary, path));
    if let Err(error) = write {
        tracing::warn!(%error, path = %path.display(), "failed to write the composed frame");
        let _ = std::fs::remove_file(&temporary);
    }
}

fn configured_output() -> (i32, i32, i32) {
    let Ok(value) = std::env::var("SOL_SCP_OUTPUT") else {
        return DEFAULT_OUTPUT;
    };
    let parsed = value.split_once(['x', 'X']).and_then(|(width, height)| {
        Some((
            width.trim().parse::<i32>().ok()?,
            height.trim().parse::<i32>().ok()?,
        ))
    });
    match parsed {
        Some((width, height)) if width > 0 && height > 0 => (width, height, DEFAULT_OUTPUT.2),
        _ => {
            tracing::warn!(%value, "ignoring malformed SOL_SCP_OUTPUT; expected WIDTHxHEIGHT");
            DEFAULT_OUTPUT
        }
    }
}

fn frame_dump_path() -> Option<PathBuf> {
    std::env::var_os("SOL_SCP_FRAME_DUMP").map(PathBuf::from)
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
