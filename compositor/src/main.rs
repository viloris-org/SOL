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
//!
//! A `--headless` mode runs the same protocol loop with no render backend at
//! all (no GPU, no display). It is driven by the integration tests and by CI,
//! which therefore do not depend on a host X/Wayland session or GL drivers.

mod grabs;
mod outputs;
mod state;
#[cfg(feature = "udev")]
mod udev_output;
#[cfg(feature = "udev")]
mod udev_runtime;
mod window;

use outputs::OutputConfiguration;
use sol_compositor::scp::ScpServer;

use std::{sync::Arc, time::Instant};

/// The clear-screen (wallpaper / window-clear) fill colour.
///
/// Matches `sol_design::DEFAULT_BACKGROUND` (0.11, 0.10, 0.13). Kept as a lone
/// constant here because the compositor does not link `sol-design` (it is a
/// client/SDK crate); the value is the canonical token, defined once in
/// `sdk/sol-design` and mirrored here. PRD §19: no bare hex in component code.
const CLEAR_BACKGROUND: smithay::backend::renderer::Color32F =
    smithay::backend::renderer::Color32F::new(0.11, 0.10, 0.13, 1.0);

use smithay::reexports::wayland_server::{
    Display, ListeningSocket,
    backend::{ClientData, ClientId, DisconnectReason},
    protocol::wl_surface::WlSurface,
};
use smithay::wayland::compositor::{
    SurfaceAttributes, TraversalAction, with_surface_tree_downward,
};
use state::{ClientState, SolState};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    } else {
        tracing_subscriber::fmt().init();
    }

    // SCP is backend-independent and remains available in winit, udev, and
    // headless modes. Keep the guard alive for the full compositor lifetime.
    let _scp_server = ScpServer::bind_from_env()?;

    // `--spawn <client>` auto-launches a Wayland client once listening, e.g.
    // `--spawn weston-terminal`. Optional; the token is otherwise ignored so
    // extra args don't break the run.
    let spawn = std::env::args()
        .position(|a| a == "--spawn")
        .and_then(|i| std::env::args().nth(i + 1));

    // `--headless` runs the protocol loop without a render backend; used by
    // the integration tests and CI (no GPU / no display required).
    let headless = std::env::args().any(|a| a == "--headless");
    let tty_udev = std::env::args().any(|a| a == "--tty-udev");

    if headless {
        return run_headless(spawn);
    }

    if tty_udev {
        #[cfg(feature = "udev")]
        {
            return run_udev(spawn);
        }
        #[cfg(not(feature = "udev"))]
        {
            tracing::error!("--tty-udev requires building with `--features udev`");
            std::process::exit(2);
        }
    }

    #[cfg(feature = "winit")]
    {
        return run_winit(spawn);
    }
    #[cfg(not(feature = "winit"))]
    {
        tracing::error!("no backend enabled; pass --headless or build with the `winit` feature");
        std::process::exit(2);
    }
    #[allow(unreachable_code)]
    Ok(())
}

/// Start the real libseat/libinput/DRM/GBM TTY backend.
#[cfg(feature = "udev")]
pub fn run_udev(spawn: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    udev_runtime::run(spawn)
}

/// Start the compositor on the winit backend (a window on the current session).
///
/// `WAYLAND_DISPLAY` / `SOL_WAYLAND_SOCKET` selects the listener socket
/// (default `wayland-sol`). `--spawn <client>` launches a Wayland client
/// against that socket once listening, for quick interactive checks
/// (e.g. `--spawn weston-terminal`).
#[cfg(feature = "winit")]
pub fn run_winit(spawn: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    use ::winit::platform::pump_events::PumpStatus;
    use smithay::{
        backend::{
            input::{AbsolutePositionEvent, InputEvent, KeyboardKeyEvent},
            renderer::{
                Frame, Renderer,
                element::{
                    Kind,
                    surface::{WaylandSurfaceRenderElement, render_elements_from_surface_tree},
                },
                gles::GlesRenderer,
                utils::draw_render_elements,
            },
            winit::{self, WinitEvent},
        },
        utils::{Rectangle, Transform},
    };

    let mut display: Display<SolState> = Display::new()?;
    let mut dh = display.handle();
    let mut state = SolState::with_output_configurations(&dh, Some(&[configured_output()?]));

    let socket_name = std::env::var("SOL_WAYLAND_SOCKET").unwrap_or_else(|_| "wayland-sol".into());
    let listener = ListeningSocket::bind(&socket_name)?;
    tracing::info!(socket = %socket_name, "SOL compositor listening");

    let (mut backend, mut winit) = winit::init::<GlesRenderer>()?;

    let keyboard = state.keyboard.clone();
    let mut serial: u32 = 0;
    let start_time = Instant::now();

    spawn_client(&spawn);

    loop {
        let status = winit.dispatch_new_events(|event| match event {
            WinitEvent::Resized { .. } => {}
            WinitEvent::Input(event) => match event {
                InputEvent::Keyboard { event } => {
                    use smithay::{
                        backend::input::{Event, KeyState},
                        input::keyboard::{FilterResult, Keysym, keysyms},
                        utils::Serial,
                    };
                    let kbd_serial = Serial::from(serial);
                    let kbd_time = event.time_msec();
                    let kbd_key = event.key_code();
                    let kbd_state = event.state();

                    // Intercept Alt+Tab (and Alt+Shift+Tab) to cycle keyboard
                    // focus between windows (Phase 1 window management) instead
                    // of forwarding the key to the focused client.
                    let tab = Keysym::from(keysyms::KEY_Tab);
                    let shift_tab = Keysym::from(keysyms::KEY_ISO_Left_Tab);
                    let action = keyboard.input::<(), _>(
                        &mut state,
                        kbd_key,
                        kbd_state,
                        kbd_serial,
                        kbd_time,
                        |_, modifiers, handle| {
                            let sym = handle.modified_sym();
                            let tab_cycle = (sym == tab || sym == shift_tab) && modifiers.alt;
                            if tab_cycle && kbd_state == KeyState::Pressed {
                                FilterResult::Intercept(())
                            } else {
                                FilterResult::Forward
                            }
                        },
                    );

                    if action.is_some() {
                        // Alt+Tab: raise & focus the next window; deliver the
                        // updated keyboard focus to that surface.
                        let surface = state.window_manager.cycle_focus();
                        keyboard.set_focus(&mut state, surface, kbd_serial);
                    }
                    serial += 1;
                }
                InputEvent::PointerMotionAbsolute { event } => {
                    use smithay::backend::input::Event;
                    // Real hit-testing: find the topmost window under the
                    // pointer and give keyboard focus to it, raising it in the
                    // z-order (Phase 1, replacing the Phase 0 "focus the first
                    // toplevel" placeholder).
                    let physical_size = backend.window_size();
                    let pos = event.position_transformed(physical_size.to_logical(1));
                    let focus = state.window_manager.surface_under(pos);
                    if let Some(ref surf) = focus {
                        state.window_manager.set_focus(surf);
                    }
                    keyboard.set_focus(&mut state, focus.clone(), serial.into());
                    serial += 1;

                    // Route the motion to the pointer handle too, so active
                    // move/resize grabs receive it and update geometry.
                    let motion = smithay::input::pointer::MotionEvent {
                        location: pos,
                        serial: serial.into(),
                        time: event.time_msec(),
                    };
                    state
                        .pointer
                        .clone()
                        .motion(&mut state, focus.map(|s| (s, pos)), &motion);
                }
                InputEvent::PointerButton { event } => {
                    use smithay::{
                        backend::input::{Event, PointerButtonEvent},
                        input::pointer::ButtonEvent,
                        utils::Serial,
                    };
                    let button = event.button_code();
                    let btn_serial = Serial::from(serial);
                    let btn_event = ButtonEvent {
                        serial: btn_serial,
                        time: event.time_msec(),
                        button,
                        state: event.state(),
                    };
                    state.pointer.clone().button(&mut state, &btn_event);
                    serial += 1;
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
        // The window manager is the single source of truth for open windows
        // (Phase 1), superseding `xdg_shell_state.toplevel_surfaces()`.
        let toplevel_surfaces: Vec<smithay::wayland::shell::xdg::ToplevelSurface> =
            state.window_manager.toplevel_surfaces().cloned().collect();

        // Render and dispatch inside a scope so the framebuffer/renderer
        // borrows of `backend` are released before `backend.submit` below.
        {
            let (renderer, mut framebuffer) = backend.bind()?;
            // HiDPI basics (PRD §33, do-not-defer): render at the primary
            // output's integer scale so a 2× output yields crisp 2× pixels.
            // Fractional scaling is verified in Phase 5.
            let scale = state.outputs.primary_scale().fractional_scale();
            let elements = toplevel_surfaces
                .iter()
                .flat_map(|surface| {
                    render_elements_from_surface_tree(
                        renderer,
                        surface.wl_surface(),
                        (0, 0),
                        scale,
                        1.0,
                        Kind::Unspecified,
                    )
                })
                .collect::<Vec<WaylandSurfaceRenderElement<GlesRenderer>>>();

            {
                let mut frame = renderer.render(&mut framebuffer, size, Transform::Flipped180)?;
                // Clear the screen to the SOL design-token background
                // (sol_design::DEFAULT_BACKGROUND, 0.11/0.10/0.13);
                // keep the compositor visually consistent with first-party UX.
                frame.clear(CLEAR_BACKGROUND, &[damage])?;
                draw_render_elements(&mut frame, scale, &elements, &[damage])?;
                let _ = frame.finish()?;
            }
        }

        for surface in &toplevel_surfaces {
            send_frames_surface_tree(
                surface.wl_surface(),
                start_time.elapsed().as_millis() as u32,
            );
        }

        accept_clients(&mut dh, &listener)?;
        display.dispatch_clients(&mut state)?;
        display.flush_clients()?;

        // Must dispatch + flush before swapping buffers; the swap can block.
        backend.submit(Some(&[damage]))?;
    }
}

/// Spawn a Wayland client against the listener socket, if requested.
fn spawn_client(spawn: &Option<String>) {
    if let Some(bin) = spawn {
        tracing::info!(%bin, "spawning test client");
        std::process::Command::new(bin).spawn().ok();
    }
}

/// Accept any clients that connected to our socket.
fn accept_clients(
    dh: &mut smithay::reexports::wayland_server::DisplayHandle,
    listener: &ListeningSocket,
) -> Result<(), Box<dyn std::error::Error>> {
    while let Some(stream) = listener.accept()? {
        tracing::debug!("accepting client");
        let _ = dh.insert_client(stream, Arc::new(ClientState::default()))?;
    }
    Ok(())
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

/// Run the compositor headless: bind the socket and service Wayland clients,
/// with no render backend. Used by integration tests and CI.
pub fn run_headless(spawn: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    let mut display: Display<SolState> = Display::new()?;
    let mut dh = display.handle();
    let mut state = SolState::with_output_configurations(&dh, Some(&[configured_output()?]));

    let socket_name = std::env::var("SOL_WAYLAND_SOCKET").unwrap_or_else(|_| "wayland-sol".into());
    let listener = ListeningSocket::bind(&socket_name)?;
    tracing::info!(socket = %socket_name, "SOL compositor listening");

    let start_time = Instant::now();

    spawn_client(&spawn);

    loop {
        // Collect the toplevel surfaces first so the borrow of `state` used
        // for rendering ends before we hand `state` to `dispatch_clients`.
        // The window manager is the single source of truth for open windows
        // (Phase 1), superseding `xdg_shell_state.toplevel_surfaces()`.
        let toplevel_surfaces: Vec<smithay::wayland::shell::xdg::ToplevelSurface> =
            state.window_manager.toplevel_surfaces().cloned().collect();

        // Send frame callbacks so committed clients can keep presenting.
        for surface in &toplevel_surfaces {
            send_frames_surface_tree(
                surface.wl_surface(),
                start_time.elapsed().as_millis() as u32,
            );
        }

        accept_clients(&mut dh, &listener)?;
        display.dispatch_clients(&mut state)?;
        display.flush_clients()?;

        // Busy-free pacing so headless CI doesn't spin a core at 100%.
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn configured_output() -> Result<OutputConfiguration, Box<dyn std::error::Error>> {
    let scale = std::env::var("SOL_OUTPUT_SCALE")
        .ok()
        .map(|value| value.parse::<f64>())
        .transpose()?;
    let configuration = OutputConfiguration::new("output-0", (1920, 1080), (0, 0));
    match scale {
        Some(scale) => Ok(configuration.try_with_scale(scale)?),
        None => Ok(configuration),
    }
}
