//! Native SCP compositor service.
//!
//! The compositor exposes only the SOL Compositor Protocol. It supports two
//! backend modes:
//!
//! - **DRM/KMS** (default): Scans out to real hardware displays via the Direct
//!   Rendering Manager kernel subsystem.
//! - **Headless** (`--headless`): Software-only composition for testing and CI,
//!   with optional frame dumps to PNG files.

use sol_compositor::{
    drm_backend::DrmBackend,
    native_input::NativeInputBackend,
    scp::{ScpServer, ScpState, protocol::OutputId},
};
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

    let running = Arc::new(AtomicBool::new(true));
    let signal_flag = Arc::clone(&running);
    ctrlc::set_handler(move || signal_flag.store(false, Ordering::Release))?;

    // Determine backend mode from command-line arguments.
    let use_headless = std::env::args().any(|arg| arg == "--headless");

    if use_headless {
        tracing::info!("starting in headless mode");
        let output_id = register_headless_output(&state)?;
        spawn_client(spawn_argument(), server.socket_path());
        tracing::info!(socket = %server.socket_path().display(), "headless SCP compositor ready");
        headless_present_loop(&state, output_id, frame_dump_path(), &running);
    } else {
        tracing::info!("starting with DRM/KMS backend");
        drm_main(&state, &running)?;
    }

    tracing::info!("native SCP compositor exiting");
    Ok(())
}

/// Main loop for the DRM/KMS backend.
fn drm_main(
    state: &Arc<Mutex<ScpState>>,
    running: &Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Open the primary DRM device. Try card0 first; fall back to card1.
    let mut backend = DrmBackend::open("/dev/dri/card0")
        .or_else(|_| DrmBackend::open("/dev/dri/card1"))
        .map_err(|e| format!("Failed to open DRM device: {e}"))?;

    // Enumerate connected displays and register them with the compositor.
    let output_ids = backend.enumerate_outputs(state)?;
    tracing::info!(outputs = output_ids.len(), "DRM outputs registered");

    let extent = backend
        .desktop_extent()
        .ok_or("DRM backend registered no usable input extent")?;
    let input = NativeInputBackend::discover(extent);
    tracing::info!(
        devices = input.device_count(),
        width = extent.0,
        height = extent.1,
        "native evdev input backend ready"
    );
    let input_state = Arc::clone(state);
    let input_running = Arc::clone(running);
    let input_thread = thread::Builder::new()
        .name("sol-native-input".to_string())
        .spawn(move || input.run(input_state, input_running))?;

    // Present frames driven by vblank events from the display hardware.
    while running.load(Ordering::Acquire) {
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        for &output_id in &output_ids {
            let framebuffer = match state.lock() {
                Ok(guard) => guard.compose_output(output_id),
                Err(_) => {
                    tracing::error!("compositor state lock is poisoned; stopping");
                    running.store(false, Ordering::Release);
                    break;
                }
            };

            let Some(ref framebuffer) = framebuffer else {
                continue;
            };
            if let Err(e) = backend.present_frame(output_id, framebuffer) {
                tracing::error!(error = %e, output_id, "failed to queue frame for presentation");
                continue;
            }
            if let Err(e) = backend.wait_for_vblank() {
                tracing::warn!(error = %e, output_id, "page flip completion failed; retaining client buffers");
                continue;
            }
            match state.lock() {
                Ok(mut guard) => guard.finish_presented_frame(timestamp_ms),
                Err(_) => {
                    tracing::error!("compositor state lock is poisoned; stopping");
                    running.store(false, Ordering::Release);
                    break;
                }
            }
        }
    }

    running.store(false, Ordering::Release);
    if input_thread.join().is_err() {
        tracing::error!("native input thread panicked during shutdown");
    }

    Ok(())
}

/// Register the headless backend's virtual output.
///
/// The headless backend has exactly one virtual display, sized from
/// `SOL_SCP_OUTPUT` (`WIDTHxHEIGHT`) so a test or developer can compose a
/// desktop at a size other than 1080p.
fn register_headless_output(
    state: &Arc<Mutex<ScpState>>,
) -> Result<OutputId, Box<dyn std::error::Error>> {
    let (width, height, refresh) = configured_output();
    let mut guard = state
        .lock()
        .map_err(|_| "compositor state lock was poisoned before the first frame")?;
    let output_id = guard
        .add_output(
            "HEADLESS-1".to_string(),
            "SOL headless output".to_string(),
            width,
            height,
            refresh,
        )
        .map_err(std::io::Error::other)?;
    tracing::info!(output_id, width, height, "headless output registered");
    Ok(output_id)
}

/// Headless presentation loop: compose and present at a fixed 60 Hz cadence.
///
/// A frame is composed on every tick rather than only when a client commits.
/// That is more work than a damage-tracking compositor needs to do, and it is
/// deliberate for now: presentation correctness — buffers released and frame
/// callbacks fired in the right order — is what this loop is here to establish,
/// and a fixed cadence makes that behavior the same on every run.
fn headless_present_loop(
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
