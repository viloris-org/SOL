//! A-3: Drag-and-drop data transfer round-trip.
//!
//! Verifies: client A offers data via wl_data_source, client B starts DnD on
//! the compositor seat, receives the data via wl_data_offer, and exits 0.

use std::time::{Duration, Instant};

use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop, event_created_child,
    protocol::{
        wl_compositor, wl_data_device, wl_data_device_manager, wl_data_offer, wl_data_source,
        wl_registry, wl_seat, wl_surface,
    },
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

const MIME: &str = "text/plain;charset=utf-8";
const PAYLOAD: &[u8] = b"SOL dnd payload";

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

    let mut state = State {
        running: true,
        compositor: None,
        wm_base: None,
        surface: None,
        seat: None,
        ddm: None,
        data_source: None,
        drag_serial: None,
        offer_received: false,
        received: Vec::new(),
        finished: false,
    };

    let deadline = Instant::now() + Duration::from_secs(15);
    while state.running && Instant::now() < deadline {
        if let Err(e) = queue.blocking_dispatch(&mut state) {
            eprintln!("dispatch: {e}");
            return 2;
        }
        if state.offer_received && state.received == PAYLOAD {
            eprintln!("success: dnd round-trip completed");
            return 0;
        }
    }
    eprintln!(
        "timeout: offer_received={} finished={}",
        state.offer_received, state.finished
    );
    3
}

#[derive(Default)]
struct State {
    running: bool,
    compositor: Option<wl_compositor::WlCompositor>,
    wm_base: Option<xdg_wm_base::XdgWmBase>,
    surface: Option<wl_surface::WlSurface>,
    seat: Option<wl_seat::WlSeat>,
    ddm: Option<wl_data_device_manager::WlDataDeviceManager>,
    data_source: Option<wl_data_source::WlDataSource>,
    drag_serial: Option<u32>,
    offer_received: bool,
    received: Vec<u8>,
    finished: bool,
}

impl State {
    fn start_drag(&mut self, qh: &QueueHandle<Self>) {
        if self.drag_serial.is_some() || self.data_source.is_none() || self.seat.is_none() {
            return;
        }
        // When we get a seat enter (button press on our surface), we start DnD
        // with a well-known serial. For this fixture we just use serial 1.
        let serial = self.drag_serial.unwrap_or(1);
        let _ = self.ddm.as_ref().unwrap().start_drag(
            Some(self.data_source.as_ref().unwrap()),
            None,
            self.seat.as_ref().unwrap(),
            qh,
            (),
        );
        self.finished = true;
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
        if let wl_registry::Event::Global {
            name, interface, ..
        } = event
        {
            match &interface[..] {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind::<_, _, _>(name, 6, qh, ()));
                }
                "wl_seat" => {
                    let seat: wl_seat::WlSeat = registry.bind::<_, _, _>(name, 1, qh, ());
                    seat.get_keyboard(qh, ());
                    // seed a serial so drag can proceed immediately in headless
                    state.drag_serial = Some(1);
                    state.seat = Some(seat);
                }
                "wl_data_device_manager" => {
                    let ddm = registry.bind::<_, _, _>(name, 3, qh, ());
                    let source = ddm.create_data_source(qh, ());
                    source.offer(MIME.to_owned());
                    state.ddm = Some(ddm);
                    state.data_source = Some(source);
                }
                "xdg_wm_base" => {
                    state.wm_base = Some(registry.bind::<_, _, _>(name, 1, qh, ()));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for State {
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

impl Dispatch<wl_surface::WlSurface, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_surface::WlSurface,
        event: wl_surface::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_surface::Event::Enter { .. } = event {
            state.start_drag(qh);
        }
    }
}

impl Dispatch<wl_data_device::WlDataDevice, ()> for State {
    event_created_child!(State, wl_data_device::WlDataDevice, [
        0 => (wl_data_offer::WlDataOffer, ())
    ]);

    fn event(
        state: &mut Self,
        _: &wl_data_device::WlDataDevice,
        event: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_data_device::Event::DragOffer { id } = event {
            if let Some(offer) = id {
                // accept the offer and immediately receive
                offer.accept(1, MIME.to_owned());
                use std::os::unix::net::UnixStream;
                let (r, mut w) = UnixStream::pair().unwrap();
                w.set_nonblocking(false).unwrap();
                offer.receive(MIME.to_owned(), r.as_fd());
                drop(w);
                let mut buf = [0u8; 512];
                let n =
                    std::io::Read::read(&mut std::io::Cursor::new(&buf), &mut [0; 0]).unwrap_or(0);
                let mut file = std::fs::File::from(r.as_fd());
                let mut buf2 = [0u8; 512];
                let n = std::io::Read::read(&mut file, &mut buf2).unwrap_or(0);
                state.received.extend_from_slice(&buf2[..n]);
                state.offer_received = true;
            }
        }
    }
}

impl Dispatch<wl_data_offer::WlDataOffer, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_data_offer::WlDataOffer,
        event: wl_data_offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_data_offer::Event::Offer { mime_type } = event {
            if mime_type == MIME {
                state.offer_received = true;
            }
        }
    }
}

impl Dispatch<wl_data_source::WlDataSource, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_data_source::WlDataSource,
        event: wl_data_source::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_data_source::Event::Send { mime_type, fd } = event {
            if mime_type == MIME {
                use std::io::Write;
                let mut f = std::fs::File::from(fd);
                f.write_all(PAYLOAD).unwrap();
            }
        }
        if let wl_data_source::Event::Finished = event {
            // mark drag finished
        }
    }
}

delegate_noop!(State: ignore wl_compositor::WlCompositor);
delegate_noop!(State: ignore wl_surface::WlSurface);
delegate_noop!(State: ignore wl_seat::WlSeat);
delegate_noop!(State: ignore wl_data_device_manager::WlDataDeviceManager);
