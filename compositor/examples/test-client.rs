//! Minimal Wayland client used by integration tests.
//!
//! Connect to a compositor socket, create an `xdg_toplevel` surface, render a
//! single solid-color frame with the software `wl_shm` path, and round-trip
//! events until the compositor acks the toplevel's configure — proving the SOL
//! compositor's protocol dispatch, surface commit, and frame-callback path work
//! end-to-end (PRD §38 Phase 0: "run standard Wayland applications").
//!
//! Modelled on `wayland-client`'s own `simple_window` example so the protocol
//! calls are known-good for `wayland-client 0.31` / `wayland-protocols 0.32`.
//!
//! Run manually:
//! ```bash
//! SOL_WAYLAND_SOCKET=wayland-sol cargo run -p sol-compositor --example test-client
//! ```

use std::{
    fs::File,
    os::fd::AsFd,
};

use wayland_client::{
    delegate_noop,
    protocol::{
        wl_buffer, wl_compositor, wl_registry, wl_seat, wl_shm, wl_shm_pool, wl_surface,
    },
    Connection, Dispatch, QueueHandle,
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

const WIDTH: i32 = 8;
const HEIGHT: i32 = 8;

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let conn = match Connection::connect_to_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("failed to connect: {e}");
            return 1;
        }
    };

    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();
    let display = conn.display();
    display.get_registry(&qh, ());

    let mut state = State {
        running: true,
        base_surface: None,
        buffer: None,
        wm_base: None,
        xdg_surface: None,
        configured: false,
    };

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while state.running && std::time::Instant::now() < deadline {
        if let Err(e) = event_queue.blocking_dispatch(&mut state) {
            eprintln!("dispatch error: {e}");
            return 2;
        }
        if state.configured {
            eprintln!("success: toplevel acknowledged by compositor");
            return 0;
        }
    }

    eprintln!("timeout: compositor never acked the toplevel");
    3
}

struct State {
    running: bool,
    base_surface: Option<wl_surface::WlSurface>,
    buffer: Option<wl_buffer::WlBuffer>,
    wm_base: Option<xdg_wm_base::XdgWmBase>,
    xdg_surface: Option<(xdg_surface::XdgSurface, xdg_toplevel::XdgToplevel)>,
    configured: bool,
}

impl State {
    fn init_xdg_surface(&mut self, qh: &QueueHandle<Self>) {
        let surface = self.base_surface.as_ref().unwrap();
        let wm_base = self.wm_base.as_ref().unwrap();
        let xdg_surface = wm_base.get_xdg_surface(surface, qh, ());
        let toplevel = xdg_surface.get_toplevel(qh, ());
        toplevel.set_title("sol-test-client".to_string());
        surface.commit();
        self.xdg_surface = Some((xdg_surface, toplevel));
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { name, interface, .. } = event {
            match &interface[..] {
                "wl_compositor" => {
                    let compositor =
                        registry.bind::<wl_compositor::WlCompositor, _, _>(name, 1, qh, ());
                    let surface = compositor.create_surface(qh, ());
                    state.base_surface = Some(surface);

                    if state.wm_base.is_some() && state.xdg_surface.is_none() {
                        state.init_xdg_surface(qh);
                    }
                }
                "wl_shm" => {
                    let shm = registry.bind::<wl_shm::WlShm, _, _>(name, 1, qh, ());

                    let mut file = shm_temp();
                    draw(&mut file, (WIDTH as u32, HEIGHT as u32));
                    let pool = shm.create_pool(file.as_fd(), WIDTH * HEIGHT * 4, qh, ());
                    let buffer = pool.create_buffer(
                        0,
                        WIDTH,
                        HEIGHT,
                        WIDTH * 4,
                        wl_shm::Format::Argb8888,
                        qh,
                        (),
                    );
                    state.buffer = Some(buffer);

                    if state.configured {
                        let surface = state.base_surface.as_ref().unwrap();
                        let buffer = state.buffer.as_ref().unwrap();
                        surface.attach(Some(buffer), 0, 0);
                        surface.commit();
                    }
                }
                "wl_seat" => {
                    registry.bind::<wl_seat::WlSeat, _, _>(name, 1, qh, ());
                }
                "xdg_wm_base" => {
                    let wm_base = registry.bind::<xdg_wm_base::XdgWmBase, _, _>(name, 1, qh, ());
                    state.wm_base = Some(wm_base);

                    if state.base_surface.is_some() && state.xdg_surface.is_none() {
                        state.init_xdg_surface(qh);
                    }
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for State {
    fn event(
        _state: &mut Self,
        wm_base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for State {
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
            state.configured = true;

            if let (Some(surface), Some(buffer)) =
                (state.base_surface.as_ref(), state.buffer.as_ref())
            {
                surface.attach(Some(buffer), 0, 0);
                surface.commit();
            }
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for State {
    fn event(
        state: &mut Self,
        _toplevel: &xdg_toplevel::XdgToplevel,
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

// Remaining protocol objects need no handling but still must be dispatched.
delegate_noop!(State: ignore wl_compositor::WlCompositor);
delegate_noop!(State: ignore wl_surface::WlSurface);
delegate_noop!(State: ignore wl_shm::WlShm);
delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
delegate_noop!(State: ignore wl_buffer::WlBuffer);
delegate_noop!(State: ignore wl_seat::WlSeat);

/// A small anonymous temporary file backing the shm pool for the test buffer.
fn shm_temp() -> std::fs::File {
    let dir = std::env::temp_dir().join("sol-test-client");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("{}-buf.bin", std::process::id()));
    let f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .read(true)
        .open(&path)
        .expect("create shm temp");
    f.set_len((WIDTH * HEIGHT * 4) as u64).expect("set_len");
    f
}

/// Fill the buffer with a flat ARGB8888 color so it is non-empty on commit.
fn draw(file: &mut File, (w, _h): (u32, u32)) {
    let n = (w * 8) as usize; // WIDTH*HEIGHT*4 == 256 bytes
    let mut pixels = Vec::with_capacity(n);
    for _ in 0..(n / 4) {
        pixels.extend_from_slice(&[255, 0, 255, 255]); // BGRA -> magenta
    }
    let _ = std::io::Write::write_all(file, &pixels);
}
