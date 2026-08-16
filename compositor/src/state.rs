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
    delegate_compositor, delegate_data_device, delegate_input_method_manager, delegate_layer_shell,
    delegate_output, delegate_seat, delegate_shm, delegate_text_input_manager, delegate_xdg_shell,
    input::{Seat, SeatHandler, SeatState, keyboard::KeyboardHandle, pointer::CursorImageStatus},
    reexports::wayland_server::{Client, DisplayHandle, Resource, protocol::wl_seat},
    utils::{SERIAL_COUNTER, Serial},
    wayland::{
        buffer::BufferHandler,
        compositor::{CompositorClientState, CompositorHandler, CompositorState},
        input_method::InputMethodHandler,
        input_method::InputMethodManagerState,
        output::OutputHandler,
        seat::WaylandFocus,
        selection::{
            SelectionHandler,
            data_device::{
                ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
                set_data_device_focus,
            },
        },
        shell::{
            wlr_layer::{
                Layer, LayerSurface as WlrLayerSurface, LayerSurfaceConfigure,
                WlrLayerShellHandler, WlrLayerShellState,
            },
            xdg::{
                Configure, PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler,
                XdgShellState,
            },
        },
        shm::{ShmHandler, ShmState},
        text_input::TextInputManagerState,
    },
};
use wayland_protocols::xdg::shell::server::xdg_toplevel;
use wayland_server::protocol::{wl_output::WlOutput, wl_surface::WlSurface};

use crate::{
    outputs::{OutputConfiguration, Outputs},
    window,
};

/// Client-level state attached to each connected Wayland client.
#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

/// Top-level compositor state shared across backends.
pub struct SolState {
    display_handle: DisplayHandle,
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub seat_state: SeatState<SolState>,
    pub data_device_state: DataDeviceState,
    pub layer_shell_state: WlrLayerShellState,
    /// Phase 1 IME: text-input v3 + input-method v2 manager globals.
    ///
    /// Kept as fields to hold the global registrations alive; the delegate
    /// macros wire the protocol dispatch into these internal manager states.
    #[allow(dead_code)]
    pub text_input_state: TextInputManagerState,
    #[allow(dead_code)]
    pub input_method_state: InputMethodManagerState,
    /// Phase 1 outputs: `wl_output` / `zxdg_output` globals + the primary
    /// output. The window manager's work area is derived from this.
    pub outputs: Outputs,
    /// Seat object retained to keep the advertised `wl_seat` global alive.
    #[allow(dead_code)]
    pub seat: Seat<SolState>,
    /// Keyboard handle shared by headless, development, and hardware backends.
    pub keyboard: KeyboardHandle<SolState>,
    /// Pointer device handle used by the interactive move/resize grabs.
    pub pointer: smithay::input::pointer::PointerHandle<SolState>,
    /// Phase 1 window management: layout, hit-testing and focus.
    pub window_manager: window::WindowManager,
}

impl SolState {
    pub fn new(display: &DisplayHandle) -> Self {
        Self::with_output_configurations(display, None)
    }

    /// Create compositor state with backend-provided output configurations.
    pub fn with_output_configurations(
        display: &DisplayHandle,
        configurations: Option<&[OutputConfiguration]>,
    ) -> Self {
        let compositor_state = CompositorState::new::<SolState>(display);
        let shm_state = ShmState::new::<SolState>(display, vec![]);
        let mut seat_state = SeatState::new();
        let mut seat = seat_state.new_wl_seat(display, "sol");
        let keyboard = seat
            .add_keyboard(Default::default(), 200, 200)
            .expect("default keyboard configuration should initialize");
        let pointer = seat.add_pointer();

        // Build the output set before the window manager so the work area can
        // be seeded from the primary output's size.
        let outputs = configurations
            .filter(|configurations| !configurations.is_empty())
            .map_or_else(
                || Outputs::new::<SolState>(display),
                |configurations| Outputs::from_configurations::<SolState>(display, configurations),
            );
        let (w, h) = outputs.primary_size();
        let mut window_manager = window::WindowManager::default();
        window_manager.set_work_area(smithay::utils::Rectangle::from_size(
            smithay::utils::Size::new(w, h),
        ));

        SolState {
            display_handle: display.clone(),
            compositor_state,
            xdg_shell_state: XdgShellState::new::<SolState>(display),
            shm_state,
            seat_state,
            data_device_state: DataDeviceState::new::<SolState>(display),
            layer_shell_state: WlrLayerShellState::new::<SolState>(display),
            text_input_state: TextInputManagerState::new::<SolState>(display),
            input_method_state: InputMethodManagerState::new::<SolState, _>(display, |_| true),
            outputs,
            seat,
            keyboard,
            pointer,
            window_manager,
        }
    }

    /// Update active output globals and keep window placement bounded by the
    /// first (primary) configured output.
    #[cfg(feature = "udev")]
    pub fn reconcile_outputs(
        &mut self,
        configurations: &[OutputConfiguration],
        display: &DisplayHandle,
    ) {
        if configurations.is_empty() {
            return;
        }
        self.outputs.reconcile::<SolState>(configurations, display);
        let (width, height) = self.outputs.primary_size();
        self.window_manager
            .set_work_area(smithay::utils::Rectangle::from_size(
                smithay::utils::Size::new(width, height),
            ));
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
        let wl_surface = surface.wl_surface().clone();
        self.window_manager.new_toplevel(surface.clone());
        surface.with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Activated);
        });
        surface.send_configure();
        self.keyboard
            .clone()
            .set_focus(self, Some(wl_surface), SERIAL_COUNTER.next_serial());
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

    /// A client requested an interactive move (`xdg_toplevel.move`).
    fn move_request(&mut self, surface: ToplevelSurface, _seat: wl_seat::WlSeat, serial: Serial) {
        // Clone the handle so we can mutate `self` while it is in scope.
        let pointer = self.pointer.clone();
        if !pointer.has_grab(serial) {
            return;
        }
        let start_data = pointer.grab_start_data().unwrap();

        // Only honor the move if the grab's focus belongs to this surface.
        let same_client = start_data
            .focus
            .as_ref()
            .map(|(s, _)| s.same_client_as(&surface.wl_surface().id()))
            .unwrap_or(false);
        if !same_client {
            return;
        }

        // Offset between the pointer and the window's top-left at grab start.
        let rect = match self
            .window_manager
            .surface_geometry(&surface.wl_surface().clone())
        {
            Some(r) => r,
            None => return,
        };
        let offset = smithay::utils::Point::from((
            start_data.location.x as i32 - rect.loc.x,
            start_data.location.y as i32 - rect.loc.y,
        ));

        let grab =
            crate::grabs::MoveSurfaceGrab::new(start_data, surface.wl_surface().clone(), offset);
        pointer.set_grab(self, grab, serial, smithay::input::pointer::Focus::Clear);
    }

    /// A client requested an interactive resize (`xdg_toplevel.resize`).
    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        _seat: wl_seat::WlSeat,
        serial: Serial,
        edges: xdg_toplevel::ResizeEdge,
    ) {
        let pointer = self.pointer.clone();
        if !pointer.has_grab(serial) {
            return;
        }
        let start_data = pointer.grab_start_data().unwrap();

        let same_client = start_data
            .focus
            .as_ref()
            .map(|(s, _)| s.same_client_as(&surface.wl_surface().id()))
            .unwrap_or(false);
        if !same_client {
            return;
        }

        let rect = match self
            .window_manager
            .surface_geometry(&surface.wl_surface().clone())
        {
            Some(r) => r,
            None => return,
        };

        let grab = crate::grabs::ResizeSurfaceGrab::new(
            start_data,
            surface.wl_surface().clone(),
            edges,
            rect,
        );
        pointer.set_grab(self, grab, serial, smithay::input::pointer::Focus::Clear);
    }
}

impl SelectionHandler for SolState {
    type SelectionUserData = ();
}

impl OutputHandler for SolState {}

impl DataDeviceHandler for SolState {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl ClientDndGrabHandler for SolState {}
impl ServerDndGrabHandler for SolState {
    fn send(&mut self, _mime_type: String, _fd: OwnedFd, _seat: Seat<Self>) {}
}

impl WlrLayerShellHandler for SolState {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell_state
    }

    fn new_layer_surface(
        &mut self,
        surface: WlrLayerSurface,
        _output: Option<WlOutput>,
        _layer: Layer,
        namespace: String,
    ) {
        tracing::debug!(%namespace, "new layer surface");

        // Provide a default size until the client resolves its own. The shell
        // top bar anchors to the full output width, but at global-creation
        // time we only know a fallback. A real implementation would read the
        // output's mode; for now give the shell a workable width.
        surface.with_pending_state(|state| {
            let size = state.size.unwrap_or_default();
            state.size = Some(smithay::utils::Size::new(
                if size.w > 0 { size.w } else { 800 },
                if size.h > 0 { size.h } else { 40 },
            ));
        });
        surface.send_configure();
    }

    fn ack_configure(&mut self, _surface: WlSurface, _configure: LayerSurfaceConfigure) {
        // Acked; nothing to do — the shell manages its own surface content.
    }
}

impl InputMethodHandler for SolState {
    fn new_popup(&mut self, _surface: smithay::wayland::input_method::PopupSurface) {
        // IME popups are rendered by `sol-ime` with `sol-design` tokens; the
        // compositor tracks them but does not need to do anything else yet.
    }

    fn dismiss_popup(&mut self, _surface: smithay::wayland::input_method::PopupSurface) {}

    fn popup_repositioned(&mut self, _surface: smithay::wayland::input_method::PopupSurface) {}

    fn parent_geometry(
        &self,
        _parent: &WlSurface,
    ) -> smithay::utils::Rectangle<i32, smithay::utils::Logical> {
        smithay::utils::Rectangle::default()
    }
}

impl SeatHandler for SolState {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<SolState> {
        &mut self.seat_state
    }

    fn focus_changed(&mut self, seat: &Seat<SolState>, focused: Option<&WlSurface>) {
        let client = focused.and_then(|surface| self.display_handle.get_client(surface.id()).ok());
        set_data_device_focus(&self.display_handle, seat, client);
    }
    fn cursor_image(&mut self, _seat: &Seat<SolState>, _image: CursorImageStatus) {}
}

// Delegate the Wayland protocol handling to the types stored in `SolState`.
delegate_compositor!(SolState);
delegate_shm!(SolState);
delegate_xdg_shell!(SolState);
delegate_seat!(SolState);
delegate_data_device!(SolState);
delegate_layer_shell!(SolState);
delegate_text_input_manager!(SolState);
delegate_input_method_manager!(SolState);
delegate_output!(SolState);
