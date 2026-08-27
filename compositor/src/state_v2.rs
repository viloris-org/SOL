//! Phase 2 compositor state: SCP-first with protocol-independent abstractions.
//!
//! This is the target architecture for Phase 2.1:
//! - `ScpState` as the single source of truth
//! - `Renderer` trait for protocol-independent rendering
//! - `InputCoordinator` for backend event → SCP message conversion
//!
//! The old `state.rs` remains temporarily for Phase 1 compatibility.

use std::sync::{Arc, Mutex};

use smithay::{
    input::{keyboard::KeyboardHandle, pointer::PointerHandle, Seat, SeatState},
    reexports::wayland_server::DisplayHandle,
    utils::SERIAL_COUNTER,
};

use crate::{
    input::InputCoordinator,
    outputs::{OutputConfiguration, Outputs},
    render::Renderer,
    scp::state::ScpState,
    window::WindowManager,
};

/// Top-level compositor state for Phase 2+.
pub struct SolStateV2 {
    display_handle: DisplayHandle,
    
    /// SCP protocol state - the single source of truth
    pub scp_state: Arc<Mutex<ScpState>>,
    
    /// Protocol-independent renderer
    pub renderer: Box<dyn Renderer>,
    
    /// Protocol-independent input coordinator
    pub input: InputCoordinator,
    
    /// Output management
    pub outputs: Outputs,
    
    /// Window management
    pub window_manager: WindowManager,
    
    // Minimal backend compatibility layer (Phase 1 transition period)
    #[allow(dead_code)]
    pub seat: Seat<Self>,
    pub seat_state: SeatState<Self>,
    pub keyboard: KeyboardHandle<Self>,
    pub pointer: PointerHandle<Self>,
    pub cursor_image: smithay::input::pointer::CursorImageStatus,
}

impl SolStateV2 {
    pub fn with_output_configurations(
        display: &DisplayHandle,
        renderer: Box<dyn Renderer>,
        configurations: Option<&[OutputConfiguration]>,
    ) -> Self {
        let mut seat_state = SeatState::new();
        let mut seat = seat_state.new_wl_seat(display, "sol");
        
        let keyboard = seat
            .add_keyboard(Default::default(), 200, 200)
            .expect("default keyboard configuration should initialize");
        
        let pointer = seat.add_pointer();
        
        let outputs = configurations
            .filter(|c| !c.is_empty())
            .map_or_else(
                || Outputs::new::<Self>(display),
                |c| Outputs::from_configurations::<Self>(display, c),
            );
        
        let (w, h) = outputs.primary_size();
        let mut window_manager = WindowManager::default();
        window_manager.set_work_area(smithay::utils::Rectangle::from_size(
            smithay::utils::Size::new(w, h),
        ));
        
        Self {
            display_handle: display.clone(),
            scp_state: Arc::new(Mutex::new(ScpState::new())),
            renderer,
            input: InputCoordinator::new(),
            outputs,
            window_manager,
            seat,
            seat_state,
            keyboard,
            pointer,
            cursor_image: smithay::input::pointer::CursorImageStatus::default_named(),
        }
    }
    
    pub fn display_handle(&self) -> &DisplayHandle {
        &self.display_handle
    }
    
    pub fn serial(&self) -> smithay::utils::Serial {
        SERIAL_COUNTER.next_serial()
    }
    
    #[cfg(feature = "udev")]
    pub fn reconcile_outputs(
        &mut self,
        configurations: &[OutputConfiguration],
        display: &DisplayHandle,
    ) {
        if configurations.is_empty() {
            return;
        }
        self.outputs.reconcile::<Self>(configurations, display);
        let (width, height) = self.outputs.primary_size();
        self.window_manager.set_work_area(
            smithay::utils::Rectangle::from_size(
                smithay::utils::Size::new(width, height),
            )
        );
    }
}

// Minimal handler implementations for backend compatibility
use smithay::wayland::compositor::{CompositorClientState, CompositorHandler};

impl smithay::wayland::buffer::BufferHandler for SolStateV2 {
    fn buffer_destroyed(&mut self, _buffer: &smithay::wayland::buffer::Buffer) {}
}

impl CompositorHandler for SolStateV2 {
    fn compositor_state(&mut self) -> &mut smithay::wayland::compositor::CompositorState {
        unimplemented!("Phase 2: compositor_state removed")
    }

    fn client_compositor_state<'a>(&self, client: &'a smithay::reexports::wayland_server::Client) -> &'a CompositorClientState {
        unimplemented!("Phase 2: client_compositor_state removed")
    }

    fn commit(&mut self, surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface) {
        // Phase 2: forward to SCP state
        // TODO: implement SCP surface commit
    }
}

smithay::delegate_compositor!(SolStateV2);
smithay::delegate_shm!(SolStateV2);
smithay::delegate_seat!(SolStateV2);
smithay::delegate_output!(SolStateV2);

impl smithay::wayland::shm::ShmHandler for SolStateV2 {
    fn shm_state(&self) -> &smithay::wayland::shm::ShmState {
        unimplemented!("Phase 2: shm_state removed")
    }
}

impl smithay::input::SeatHandler for SolStateV2 {
    type KeyboardFocus = smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
    type PointerFocus = smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
    type TouchFocus = smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }

    fn focus_changed(&mut self, _seat: &Seat<Self>, _focused: Option<&Self::KeyboardFocus>) {}
    fn cursor_image(&mut self, _seat: &Seat<Self>, image: smithay::input::pointer::CursorImageStatus) {
        self.cursor_image = image;
    }
}

impl smithay::wayland::output::OutputHandler for SolStateV2 {}
