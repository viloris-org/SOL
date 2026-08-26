//! SOL Desktop Shell
//!
//! The shell is a **separate process** from `sol-compositor` (PRD §11). It
//! renders system UI over the compositor-provided layer-shell surface.
//!
//! ## Phase 1 M1 deliverable
//!
//! This milestone ships the **first shell surface**: a layer-shell top bar that
//! connects to the compositor, gets a configure, and renders a solid bar using
//! `sol-design` tokens — no hand-written visual parameters (PRD §19.1).
//!
//! A shell crash must not take down the compositor — they are separate
//! processes joined by the Wayland layer-shell protocol (and the compositor↔
//! shell D-Bus IPC from ADR-0006).

mod client;

use std::{
    fs::File,
    io::{Seek, SeekFrom, Write},
    os::fd::AsFd,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use tracing::level_filters::LevelFilter;
use wayland_client::{
    Dispatch, Proxy, QueueHandle,
    protocol::{
        wl_buffer::WlBuffer, wl_registry, wl_shm, wl_shm_pool::WlShmPool, wl_surface::WlSurface,
    },
};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{Layer, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::{Anchor, KeyboardInteractivity, ZwlrLayerSurfaceV1},
};

use client::Globals;
use sol_design::color::Color;
use sol_diagnostics::{DiagnosticSource, SolComponent, install_default_panic_capture};
use sol_scheduler::{SHELL_RT_PRIORITY, promote_current_thread};

/// The logical height of the top bar.
const BAR_HEIGHT: i32 = 40;

/// The live shell state, driving both Wayland dispatch and rendering.
pub struct Shell {
    /// The wl_surface underlying the layer surface.
    surface: WlSurface,
    /// The layer-shell surface we anchor to the top edge.
    ///
    /// Kept as a field to hold the proxy alive; the `Dispatch<ZwlrLayerSurfaceV1>`
    /// impl below handles its events (the configure / close), so the field is
    /// only referenced indirectly.
    #[allow(dead_code)]
    layer_surface: ZwlrLayerSurfaceV1,
    /// Backing file for the shm pool (resized when the compositor gives us a
    /// new size via Configure).
    pool_file: File,
    /// The wl_shm_pool proxy that owns the fd exposed to the compositor.
    pool: WlShmPool,
    /// Queue handle for creating new buffer objects against this event queue.
    qh: QueueHandle<Self>,
    /// Current render dimensions.
    width: i32,
    height: i32,
    /// Whether the compositor has sent its initial Configure. Until then we
    /// don't render.
    configured: bool,
    /// A flag integration tests / callers can read to confirm the shell has
    /// committed at least one frame.
    pub committed: bool,
}

// -- Dispatch impls ---------------------------------------------------------

impl Dispatch<wl_registry::WlRegistry, ()> for Shell {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &(),
        _: &wayland_client::Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSurface, ()> for Shell {
    fn event(
        _: &mut Self,
        _: &WlSurface,
        _: <WlSurface as Proxy>::Event,
        _: &(),
        _: &wayland_client::Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, ()> for Shell {
    fn event(
        state: &mut Self,
        surface: &ZwlrLayerSurfaceV1,
        event: <ZwlrLayerSurfaceV1 as Proxy>::Event,
        _: &(),
        _: &wayland_client::Connection,
        _: &QueueHandle<Self>,
    ) {
        use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::Event;
        match event {
            Event::Configure {
                serial,
                width,
                height,
            } => {
                surface.ack_configure(serial);
                if width > 0 {
                    state.width = width as i32;
                }
                if height > 0 {
                    state.height = height as i32;
                }
                tracing::debug!(width = state.width, height = state.height, "configured");
                state.configured = true;
            }
            Event::Closed => {
                tracing::info!("layer surface closed, shell exiting");
                std::process::exit(0);
            }
            _ => {}
        }
    }
}

impl Dispatch<wayland_client::protocol::wl_compositor::WlCompositor, ()> for Shell {
    fn event(
        _: &mut Self,
        _: &wayland_client::protocol::wl_compositor::WlCompositor,
        _: <wayland_client::protocol::wl_compositor::WlCompositor as Proxy>::Event,
        _: &(),
        _: &wayland_client::Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_shm::WlShm, ()> for Shell {
    fn event(
        _: &mut Self,
        _: &wl_shm::WlShm,
        _: <wl_shm::WlShm as Proxy>::Event,
        _: &(),
        _: &wayland_client::Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlShmPool, ()> for Shell {
    fn event(
        _: &mut Self,
        _: &WlShmPool,
        _: <WlShmPool as Proxy>::Event,
        _: &(),
        _: &wayland_client::Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlBuffer, ()> for Shell {
    fn event(
        _: &mut Self,
        _: &WlBuffer,
        _: <WlBuffer as Proxy>::Event,
        _: &(),
        _: &wayland_client::Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrLayerShellV1, ()> for Shell {
    fn event(
        _: &mut Self,
        _: &ZwlrLayerShellV1,
        _: <ZwlrLayerShellV1 as Proxy>::Event,
        _: &(),
        _: &wayland_client::Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

// -- Construction + rendering -----------------------------------------------

impl Shell {
    /// Create the top-bar surface from the compositor's globals.
    pub fn new(
        globals: &Globals,
        qh: &QueueHandle<Self>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let surface = globals.compositor.create_surface(qh, ());
        let layer_surface = globals.layer_shell.get_layer_surface(
            &surface,
            None, // primary output
            Layer::Top,
            "sol-shell".to_string(),
            qh,
            (),
        );

        layer_surface.set_anchor(Anchor::Top);
        layer_surface.set_exclusive_zone(BAR_HEIGHT);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer_surface.set_size(480u32, BAR_HEIGHT as u32);

        // Create an initial shm pool large enough for the fallback size.
        let (pool_file, pool) = alloc_shm_pool(&globals.shm, qh, 480, BAR_HEIGHT);

        Ok(Shell {
            surface,
            layer_surface,
            pool_file,
            pool,
            qh: qh.clone(),
            width: 480,
            height: BAR_HEIGHT,
            configured: false,
            committed: false,
        })
    }

    /// Fill the buffer with the `Color::Elevated` token and commit a frame.
    pub fn render_frame(&mut self) {
        if !self.configured {
            return;
        }

        let rgba = Color::Elevated.rgba();
        let stride = self.width * 4;
        let buf_size = (stride * self.height) as u64;
        if self.pool_file.metadata().map(|m| m.len()).unwrap_or(0) < buf_size {
            let _ = self.pool_file.set_len(buf_size);
        }
        let _ = self.pool_file.seek(SeekFrom::Start(0));
        let mut row = vec![0u8; stride as usize];
        for chunk in row.chunks_mut(4) {
            chunk.copy_from_slice(&[
                (rgba.0 * 255.0) as u8,
                (rgba.1 * 255.0) as u8,
                (rgba.2 * 255.0) as u8,
                (rgba.3 * 255.0) as u8,
            ]);
        }
        for _ in 0..self.height {
            let _ = self.pool_file.write_all(&row);
        }

        let buffer = self.pool.create_buffer(
            0,
            self.width,
            self.height,
            stride,
            wl_shm::Format::Argb8888,
            &self.qh,
            (),
        );
        self.surface.attach(Some(&buffer), 0, 0);
        self.surface.commit();
        self.committed = true;
    }
}

/// Allocate a `wl_shm_pool` backed by an anonymous temp file of the given size.
fn alloc_shm_pool(
    shm: &wl_shm::WlShm,
    qh: &QueueHandle<Shell>,
    width: i32,
    height: i32,
) -> (File, WlShmPool) {
    let size = width * height * 4;
    let path = std::path::PathBuf::from("/tmp/sol-shell-buffer");
    // Open create/truncate with the requested size, then hand the fd to the
    // compositor for mmap. The file must already be sized or `wl_shm` mmap
    // will fail.
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .read(true)
        .open(&path)
        .expect("create shm backing file");
    let _ = file.set_len(size as u64);
    let fd = file.as_fd();
    let pool = shm.create_pool(fd, size, qh, ());
    (file, pool)
}

// -- Entry point -------------------------------------------------------------

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::DEBUG)
        .init();

    if let Err(error) =
        install_default_panic_capture(DiagnosticSource::Component(SolComponent::Shell))
    {
        tracing::warn!(%error, "shell crash capture is unavailable");
    }

    let once = std::env::args().any(|a| a == "--once");
    tracing::info!(once, "sol-shell starting");

    if let Err(error) = promote_current_thread(SHELL_RT_PRIORITY) {
        tracing::warn!(%error, "SCHED_FIFO priority 1 unavailable; shell UI loop remains on CFS");
    } else {
        tracing::info!("shell UI event loop elevated to SCHED_FIFO priority 1");
    }

    let mut client = client::ShellClient::connect()?;
    let mut shell = Shell::new(&client.globals, &client.qh)?;

    if once {
        // Single-shot mode for integration tests / CI: pump until the
        // compositor sends the initial Configure, render one frame, and exit
        // with status 0 only if we actually committed. This lets a test drive
        // the round-trip deterministically without a long-running process.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !shell.configured && std::time::Instant::now() < deadline {
            client.pump(&mut shell)?;
        }
        if !shell.configured {
            tracing::error!("timed out waiting for layer-surface configure");
            std::process::exit(1);
        }
        shell.render_frame();
        if shell.committed {
            tracing::info!("sol-shell round-trip OK (frame committed)");
            std::process::exit(0);
        }
        tracing::error!("shell never committed a frame");
        std::process::exit(1);
    }

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("set Ctrl-C handler");

    while running.load(Ordering::SeqCst) {
        client.pump(&mut shell)?;
        shell.render_frame();
        std::thread::sleep(std::time::Duration::from_millis(16));
    }

    tracing::info!("sol-shell exiting cleanly");
    Ok(())
}
