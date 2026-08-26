//! A-2: xdg_surface popup round-trip.
//!
//! Creates a toplevel, then a popup anchored to it, and verifies the compositor
//! delivers configure events for both. Exits 0 on success.

use std::time::{Duration, Instant};

use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    protocol::{wl_compositor, wl_registry, wl_seat, wl_surface},
};
use wayland_protocols::xdg::shell::client::{
    xdg_positioner, xdg_surface, xdg_toplevel, xdg_wm_base,
};

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let conn = match Connection::connect_to_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("connect: {e}");
            return 1;
        }
    };
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());

    let mut state = PopupState {
        running: true,
        compositor: None,
        wm_base: None,
        toplevel_xdg: None,
        popup_created: false,
        toplevel_configured: false,
        popup_configured: false,
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while state.running && Instant::now() < deadline {
        if let Err(e) = queue.blocking_dispatch(&mut state) {
            eprintln!("dispatch: {e}");
            return 2;
        }
        if state.toplevel_configured && state.popup_configured {
            eprintln!("success: popup round-trip OK");
            return 0;
        }
    }
    eprintln!(
        "timeout: toplevel={} popup={}",
        state.toplevel_configured, state.popup_configured
    );
    3
}

struct PopupState {
    running: bool,
    compositor: Option<wl_compositor::WlCompositor>,
    wm_base: Option<xdg_wm_base::XdgWmBase>,
    toplevel_xdg: Option<xdg_surface::XdgSurface>,
    popup_created: bool,
    toplevel_configured: bool,
    popup_configured: bool,
}

impl PopupState {
    fn try_create_popup(&mut self, qh: &QueueHandle<Self>) {
        if self.popup_created {
            return;
        }
        let (Some(compositor), Some(wm_base), Some(parent)) =
            (&self.compositor, &self.wm_base, &self.toplevel_xdg)
        else {
            return;
        };
        self.popup_created = true;

        let positioner = wm_base.create_positioner(qh, ());
        positioner.set_size(100, 50);
        positioner.set_anchor_rect(0, 0, 1, 1);
        positioner.set_anchor(xdg_positioner::Anchor::TopLeft);
        positioner.set_gravity(xdg_positioner::Gravity::BottomRight);

        let popup_surface = compositor.create_surface(qh, ());
        let popup_xdg = wm_base.get_xdg_surface(&popup_surface, qh, ());
        popup_xdg.get_popup(Some(parent), &positioner, qh, ());
        popup_surface.commit();
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for PopupState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name, interface, ..
        } = event
        {
            match &interface[..] {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind::<_, _, _>(name, 6, qh, ()));
                }
                "wl_seat" => {
                    registry.bind::<wl_seat::WlSeat, _, _>(name, 1, qh, ());
                }
                "xdg_wm_base" => {
                    state.wm_base = Some(registry.bind::<_, _, _>(name, 1, qh, ()));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for PopupState {
    fn event(
        _: &mut Self,
        wb: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wb.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for PopupState {
    fn event(
        state: &mut Self,
        xdg_surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial, .. } = event {
            xdg_surface.ack_configure(serial);
            if state.toplevel_xdg.is_none() {
                state.toplevel_xdg = Some(xdg_surface.clone());
                state.toplevel_configured = true;
                state.try_create_popup(qh);
            } else {
                state.popup_configured = true;
            }
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for PopupState {
    fn event(
        state: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_toplevel::Event::Close = event {
            state.running = false;
        }
    }
}

delegate_noop!(PopupState: ignore wl_compositor::WlCompositor);
delegate_noop!(PopupState: ignore wl_surface::WlSurface);
delegate_noop!(PopupState: ignore wl_seat::WlSeat);
delegate_noop!(PopupState: ignore xdg_wm_base::XdgWmBase);
