//! SOL compositor — Phase 0
//!
//! A functional Smithay compositor that can start a standalone Wayland
//! session (PRD §38 Phase 0 success criterion):
//!
//! ```text
//! > 能启动独立 SOL Wayland Session，并运行标准 Wayland 应用
//! ```
//!
//! For development this runs on Smithay's `winit` backend, which renders into
//! a normal window on the surrounding Wayland/X11 session — no DRM grab
//! required, so it can run without leaving the current desk or being root.
//! The same [`SolState`] core is reused by the udev/DRM backend
//! (`features = ["udev"]`, `--tty-udev`) for real hardware sessions later.
//!
//! This milestone intentionally ships the "minimal well-formed compositor":
//! the wire protocols every client needs (`wl_compositor`, `wl_shm`,
//! `xdg_shell`, seat, data-device) plus the render/frame-callback loop. Window
//! management, workspaces, layer-shell (shell) and XWayland are Phase 1.

mod state;

use std::{sync::Arc, time::Instant};

use ::winit::platform::pump_events::PumpStatus;
use smithay::{
    backend::{
        input::{InputEvent, KeyboardKeyEvent},
        renderer::{
            Color32F, Frame, Renderer,
            element::{
                Kind,
                surface::{WaylandSurfaceRenderElement, render_elements_from_surface_tree},
            },
            gles::GlesRenderer,
            utils::draw_render_elements,
        },
        winit::{self, WinitEvent},
    },
    input::keyboard::FilterResult,
    reexports::wayland_server::{
        Display, ListeningSocket,
        backend::{ClientData, ClientId, DisconnectReason},
        protocol::wl_surface::WlSurface,
    },
    utils::{Rectangle, Transform},
    wayland::compositor::{SurfaceAttributes, TraversalAction, with_surface_tree_downward},
};
use state::{ClientState, SolState};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    } else {
        tracing_subscriber::fmt().init();
    }

    // `--spawn <client>` auto-launches a Wayland client once listening, e.g.
    // `--spawn weston-terminal`. Optional; the token is otherwise ignored so
    // extra args don't break the run.
    let spawn = std::env::args()
        .position(|a| a == "--spawn")
        .and_then(|i| std::env::args().nth(i + 1));

    run_winit(spawn)?;
    Ok(())
}

/// Start the compositor on the winit backend (a window on the current session).
///
/// `WAYLAND_DISPLAY` / `SOL_WAYLAND_SOCKET` selects the listener socket
/// (default `wayland-sol`). `--spawn <client>` launches a Wayland client
/// against that socket once listening, for quick interactive checks
/// (e.g. `--spawn weston-terminal`).
pub fn run_winit(spawn: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    let mut display: Display<SolState> = Display::new()?;
    let mut dh = display.handle();
    let mut state = SolState::new(&dh);

    let socket_name = std::env::var("SOL_WAYLAND_SOCKET").unwrap_or_else(|_| "wayland-sol".into());
    let listener = ListeningSocket::bind(&socket_name)?;
    tracing::info!(socket = %socket_name, "SOL compositor listening");

    let (mut backend, mut winit) = winit::init::<GlesRenderer>()?;

    let keyboard = state.seat.add_keyboard(Default::default(), 200, 200)?;
    let start_time = Instant::now();

    if let Some(bin) = spawn {
        tracing::info!(%bin, "spawning test client");
        std::process::Command::new(&bin).spawn().ok();
    }

    loop {
        let status = winit.dispatch_new_events(|event| match event {
            WinitEvent::Resized { .. } => {}
            WinitEvent::Input(event) => match event {
                InputEvent::Keyboard { event } => {
                    keyboard.input::<(), _>(
                        &mut state,
                        event.key_code(),
                        event.state(),
                        0.into(),
                        0,
                        |_, _, _| FilterResult::Forward,
                    );
                }
                InputEvent::PointerMotionAbsolute { .. } => {
                    // Give keyboard input somewhere to go: focus the first
                    // toplevel on any pointer motion. Real focus handling is
                    // Phase 1 window management.
                    let focus = state
                        .xdg_shell_state
                        .toplevel_surfaces()
                        .first()
                        .map(|s| s.wl_surface().clone());
                    keyboard.set_focus(&mut state, focus, 0.into());
                }
                _ => {}
            },
            _ => (),
        });

        match status {
            PumpStatus::Continue => (),
            PumpStatus::Exit(_) => return Ok(()),
        };

        let size = backend.window_size();
        let damage = Rectangle::from_size(size);

        // Collect the toplevel surfaces first so the borrow of `state` used
        // for rendering ends before we hand `state` to `dispatch_clients`.
        let toplevel_surfaces: Vec<smithay::wayland::shell::xdg::ToplevelSurface> =
            state.xdg_shell_state.toplevel_surfaces().to_vec();

        // Render and dispatch inside a scope so the framebuffer/renderer
        // borrows of `backend` are released before `backend.submit` below.
        {
            let (renderer, mut framebuffer) = backend.bind()?;
            let elements = toplevel_surfaces
                .iter()
                .flat_map(|surface| {
                    render_elements_from_surface_tree(
                        renderer,
                        surface.wl_surface(),
                        (0, 0),
                        1.0,
                        1.0,
                        Kind::Unspecified,
                    )
                })
                .collect::<Vec<WaylandSurfaceRenderElement<GlesRenderer>>>();

            {
                let mut frame = renderer.render(&mut framebuffer, size, Transform::Flipped180)?;
                frame.clear(Color32F::new(0.11, 0.10, 0.13, 1.0), &[damage])?;
                draw_render_elements(&mut frame, 1.0, &elements, &[damage])?;
                let _ = frame.finish()?;
            }
        }

        for surface in &toplevel_surfaces {
            send_frames_surface_tree(
                surface.wl_surface(),
                start_time.elapsed().as_millis() as u32,
            );
        }

        // Accept any clients that connected to our socket.
        while let Some(stream) = listener.accept()? {
            tracing::debug!("accepting client");
            let _ = dh.insert_client(stream, Arc::new(ClientState::default()))?;
        }

        display.dispatch_clients(&mut state)?;
        display.flush_clients()?;

        // Must dispatch + flush before swapping buffers; the swap can block.
        backend.submit(Some(&[damage]))?;
    }
}

/// Send frame callbacks to every surface in the tree, so clients repaint.
pub fn send_frames_surface_tree(surface: &WlSurface, time: u32) {
    with_surface_tree_downward(
        surface,
        (),
        |_, _, &()| TraversalAction::DoChildren(()),
        |_surf, states, &()| {
            for callback in states
                .cached_state
                .get::<SurfaceAttributes>()
                .current()
                .frame_callbacks
                .drain(..)
            {
                callback.done(time);
            }
        },
        |_, _, &()| true,
    );
}

impl ClientData for ClientState {
    fn initialized(&self, client_id: ClientId) {
        tracing::debug!(?client_id, "client initialized");
    }

    fn disconnected(&self, client_id: ClientId, _reason: DisconnectReason) {
        tracing::debug!(?client_id, "client disconnected");
    }
}
