//! Core SOL compositor state.
//!
//! `SolState` owns the Smithay protocol state (compositor, shm, xdg-shell,
//! seat, data-device) that a functioning Wayland compositor must expose, and
//! implements the handlers that drive them. Backends (winit for development,
//! udev/DRM for real hardware) drive this state from a single event loop.
//!
//! This is deliberately modelled on Smithay's `examples/minimal` — the
//! minimal well-formed compositor — so we validate the whole graphic stack
//! (protocol dispatch, seat/keyboard, rendering, client lifecycle) before any
//! window management or shell work begins (PRD §38 Phase 0).

use std::os::unix::io::OwnedFd;

use smithay::{
    delegate_compositor, delegate_data_device, delegate_seat, delegate_shm, delegate_xdg_shell,
    input::{Seat, SeatHandler, SeatState, pointer::CursorImageStatus},
    reexports::wayland_server::{Client, DisplayHandle, protocol::wl_seat},
    utils::Serial,
    wayland::{
        buffer::BufferHandler,
        compositor::{CompositorClientState, CompositorHandler, CompositorState},
        selection::{
            SelectionHandler,
            data_device::{
                ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
            },
        },
        shell::xdg::{
            Configure, PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler,
            XdgShellState,
        },
        shm::{ShmHandler, ShmState},
    },
};
use wayland_protocols::xdg::shell::server::xdg_toplevel;
use wayland_server::protocol::wl_surface::WlSurface;

use crate::window;

/// Client-level state attached to each connected Wayland client.
#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

/// Top-level compositor state shared across backends.
pub struct SolState {
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub seat_state: SeatState<SolState>,
    pub data_device_state: DataDeviceState,
    pub seat: Seat<SolState>,
    /// Phase 1 window management: layout, hit-testing and focus.
    pub window_manager: window::WindowManager,
}

impl SolState {
    pub fn new(display: &DisplayHandle) -> Self {
        let compositor_state = CompositorState::new::<SolState>(display);
        let shm_state = ShmState::new::<SolState>(display, vec![]);
        let mut seat_state = SeatState::new();
        let seat = seat_state.new_wl_seat(display, "sol");

        SolState {
            compositor_state,
            xdg_shell_state: XdgShellState::new::<SolState>(display),
            shm_state,
            seat_state,
            data_device_state: DataDeviceState::new::<SolState>(display),
            seat,
            window_manager: window::WindowManager::default(),
        }
    }
}

impl BufferHandler for SolState {
    fn buffer_destroyed(&mut self, _buffer: &wayland_server::protocol::wl_buffer::WlBuffer) {}
}

impl CompositorHandler for SolState {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client
            .get_data::<ClientState>()
            .expect("clients carry SolState::ClientState")
            .compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        smithay::backend::renderer::utils::on_commit_buffer_handler::<Self>(surface);
    }
}

impl ShmHandler for SolState {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

impl XdgShellHandler for SolState {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        // Track the window and set it as the focused, activated toplevel.
        self.window_manager.new_toplevel(surface.clone());
        surface.with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Activated);
        });
        surface.send_configure();
    }

    /// A client acknowledged a configure serial; sync the window's size with
    /// what the client actually committed to.
    fn ack_configure(&mut self, surface: WlSurface, configure: Configure) {
        let size = match configure {
            Configure::Toplevel(config) => config.state.size,
            Configure::Popup(_) => None,
        };
        if let Some(size) = size {
            self.window_manager.update_size(&surface, size);
        }
    }

    fn new_popup(&mut self, _surface: PopupSurface, _positioner: PositionerState) {}
    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {}
    fn reposition_request(
        &mut self,
        _surface: PopupSurface,
        _positioner: PositionerState,
        _token: u32,
    ) {
    }
}

impl SelectionHandler for SolState {
    type SelectionUserData = ();
}

impl DataDeviceHandler for SolState {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl ClientDndGrabHandler for SolState {}
impl ServerDndGrabHandler for SolState {
    fn send(&mut self, _mime_type: String, _fd: OwnedFd, _seat: Seat<Self>) {}
}

impl SeatHandler for SolState {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<SolState> {
        &mut self.seat_state
    }

    fn focus_changed(&mut self, _seat: &Seat<SolState>, _focused: Option<&WlSurface>) {}
    fn cursor_image(&mut self, _seat: &Seat<SolState>, _image: CursorImageStatus) {}
}

// Delegate the Wayland protocol handling to the types stored in `SolState`.
delegate_compositor!(SolState);
delegate_shm!(SolState);
delegate_xdg_shell!(SolState);
delegate_seat!(SolState);
delegate_data_device!(SolState);
