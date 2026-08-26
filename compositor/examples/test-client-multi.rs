//! A-1: Multiple concurrent xdg_toplevel round-trip.
//!
//! Creates 3 toplevel surfaces and waits for all 3 to receive configure acks.

use std::time::{Duration, Instant};

use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    protocol::{wl_compositor, wl_registry, wl_seat, wl_surface},
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

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

    let mut state = MultiState {
        running: true,
        compositor: None,
        wm_base: None,
        acked: 0,
        toplevels_created: false,
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while state.running && Instant::now() < deadline {
        if let Err(e) = queue.blocking_dispatch(&mut state) {
            eprintln!("dispatch: {e}");
            return 2;
        }
        if state.acked == 3 {
            eprintln!("success: 3 toplevels acked");
            return 0;
        }
    }
    eprintln!("timeout: acked={}/3", state.acked);
    3
}

struct MultiState {
    running: bool,
    compositor: Option<wl_compositor::WlCompositor>,
    wm_base: Option<xdg_wm_base::XdgWmBase>,
    acked: usize,
    toplevels_created: bool,
}

impl MultiState {
    fn try_create_toplevels(&mut self, qh: &QueueHandle<Self>) {
        if self.toplevels_created || self.compositor.is_none() || self.wm_base.is_none() {
            return;
        }
        self.toplevels_created = true;
        for i in 0..3 {
            let surface = self.compositor.as_ref().unwrap().create_surface(qh, ());
            let xdg_surface = self
                .wm_base
                .as_ref()
                .unwrap()
                .get_xdg_surface(&surface, qh, ());
            xdg_surface
                .get_toplevel(qh, ())
                .set_title(format!("multi-{i}"));
            surface.commit();
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for MultiState {
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
                    state.compositor = Some(registry.bind::<_, _, _>(name, 1, qh, ()));
                    state.try_create_toplevels(qh);
                }
                "wl_seat" => {
                    registry.bind::<wl_seat::WlSeat, _, _>(name, 1, qh, ());
                }
                "xdg_wm_base" => {
                    state.wm_base = Some(registry.bind::<_, _, _>(name, 1, qh, ()));
                    state.try_create_toplevels(qh);
                }
                _ => {}
            }
        }
    }
}

// WmBase and xdg_surface/xdg_toplevel are handled by delegate_noop.
// We count acked configures via the xdg_surface dispatch below.

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for MultiState {
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

impl Dispatch<xdg_surface::XdgSurface, ()> for MultiState {
    fn event(
        state: &mut Self,
        xdg_surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial, .. } = event {
            xdg_surface.ack_configure(serial);
            state.acked += 1;
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for MultiState {
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

// Do NOT delegate_noop for XdgWmBase/XdgSurface/XdgToplevel since we implement
// Dispatch for them above. Only delegate the remaining unused types.
delegate_noop!(MultiState: ignore wl_compositor::WlCompositor);
delegate_noop!(MultiState: ignore wl_surface::WlSurface);
delegate_noop!(MultiState: ignore wl_seat::WlSeat);
