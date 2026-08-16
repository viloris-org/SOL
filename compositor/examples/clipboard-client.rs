//! Wayland clipboard fixture for the headless SOL compositor.

use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

use wayland_client::protocol::{
    wl_compositor, wl_data_device, wl_data_device_manager, wl_data_offer, wl_data_source,
    wl_keyboard, wl_registry, wl_seat, wl_surface,
};
use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop, event_created_child};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

const MIME_TYPE: &str = "text/plain;charset=utf-8";
const PAYLOAD: &[u8] = b"SOL native Wayland clipboard";

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let connection = match Connection::connect_to_env() {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("failed to connect: {error}");
            return 1;
        }
    };
    let mut queue = connection.new_event_queue();
    let handle = queue.handle();
    connection.display().get_registry(&handle, ());
    let mut state = State::default();
    let deadline = Instant::now() + Duration::from_secs(10);

    while Instant::now() < deadline {
        if let Err(error) = queue.blocking_dispatch(&mut state) {
            eprintln!("dispatch error: {error}");
            return 2;
        }
        if state.read_payload() {
            eprintln!("success: clipboard selection round-trip completed");
            return 0;
        }
        state.maybe_set_selection();
    }

    eprintln!(
        "timeout: selection_set={}, offered={}, received={:?}",
        state.selection_set,
        state.offered_text,
        String::from_utf8_lossy(&state.received)
    );
    3
}

#[derive(Default)]
struct State {
    surface: Option<wl_surface::WlSurface>,
    wm_base: Option<xdg_wm_base::XdgWmBase>,
    xdg_surface: Option<xdg_surface::XdgSurface>,
    seat: Option<wl_seat::WlSeat>,
    data_device_manager: Option<wl_data_device_manager::WlDataDeviceManager>,
    data_device: Option<wl_data_device::WlDataDevice>,
    data_source: Option<wl_data_source::WlDataSource>,
    keyboard_serial: Option<u32>,
    selection_set: bool,
    offered_text: bool,
    reader: Option<UnixStream>,
    received: Vec<u8>,
}

impl State {
    fn init_xdg_surface(&mut self, handle: &QueueHandle<Self>) {
        if self.xdg_surface.is_some() {
            return;
        }
        let (Some(surface), Some(wm_base)) = (&self.surface, &self.wm_base) else {
            return;
        };
        let xdg_surface = wm_base.get_xdg_surface(surface, handle, ());
        let toplevel = xdg_surface.get_toplevel(handle, ());
        toplevel.set_title("sol-clipboard-client".to_owned());
        surface.commit();
        self.xdg_surface = Some(xdg_surface);
    }

    fn init_data_device(&mut self, handle: &QueueHandle<Self>) {
        if self.data_device.is_some() {
            return;
        }
        let (Some(manager), Some(seat)) = (&self.data_device_manager, &self.seat) else {
            return;
        };
        self.data_device = Some(manager.get_data_device(seat, handle, ()));
    }

    fn maybe_set_selection(&mut self) {
        if self.selection_set {
            return;
        }
        let (Some(device), Some(source), Some(serial)) =
            (&self.data_device, &self.data_source, self.keyboard_serial)
        else {
            return;
        };
        device.set_selection(Some(source), serial);
        self.selection_set = true;
    }

    fn receive_offer(&mut self, offer: &wl_data_offer::WlDataOffer) {
        if !self.offered_text || self.reader.is_some() {
            return;
        }
        let (reader, writer) = UnixStream::pair().expect("create clipboard transfer socket");
        reader
            .set_nonblocking(true)
            .expect("make clipboard reader nonblocking");
        offer.receive(MIME_TYPE.to_owned(), writer.as_fd());
        drop(writer);
        self.reader = Some(reader);
    }

    fn read_payload(&mut self) -> bool {
        let Some(reader) = &mut self.reader else {
            return false;
        };
        let mut buffer = [0_u8; 128];
        match reader.read(&mut buffer) {
            Ok(0) => self.received == PAYLOAD,
            Ok(length) => {
                self.received.extend_from_slice(&buffer[..length]);
                self.received == PAYLOAD
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => false,
            Err(error) => {
                eprintln!("clipboard read failed: {error}");
                false
            }
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        else {
            return;
        };
        match interface.as_str() {
            "wl_compositor" => {
                let compositor = registry.bind::<wl_compositor::WlCompositor, _, _>(
                    name,
                    version.min(6),
                    handle,
                    (),
                );
                state.surface = Some(compositor.create_surface(handle, ()));
                state.init_xdg_surface(handle);
            }
            "wl_seat" => {
                let seat = registry.bind::<wl_seat::WlSeat, _, _>(name, version.min(9), handle, ());
                seat.get_keyboard(handle, ());
                state.seat = Some(seat);
                state.init_data_device(handle);
            }
            "wl_data_device_manager" => {
                let manager = registry.bind::<wl_data_device_manager::WlDataDeviceManager, _, _>(
                    name,
                    version.min(3),
                    handle,
                    (),
                );
                let source = manager.create_data_source(handle, ());
                source.offer(MIME_TYPE.to_owned());
                state.data_device_manager = Some(manager);
                state.data_source = Some(source);
                state.init_data_device(handle);
            }
            "xdg_wm_base" => {
                state.wm_base =
                    Some(registry.bind::<xdg_wm_base::XdgWmBase, _, _>(name, 1, handle, ()));
                state.init_xdg_surface(handle);
            }
            _ => {}
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for State {
    fn event(
        _: &mut Self,
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
        _: &mut Self,
        xdg_surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            xdg_surface.ack_configure(serial);
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Enter { serial, .. } = event {
            state.keyboard_serial = Some(serial);
            state.maybe_set_selection();
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
        if let wl_data_device::Event::Selection { id: Some(offer) } = event {
            state.receive_offer(&offer);
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
            state.offered_text |= mime_type == MIME_TYPE;
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
            assert_eq!(mime_type, MIME_TYPE);
            let mut file = File::from(fd);
            file.write_all(PAYLOAD)
                .expect("write clipboard fixture payload");
        }
    }
}

delegate_noop!(State: ignore wl_compositor::WlCompositor);
delegate_noop!(State: ignore wl_surface::WlSurface);
delegate_noop!(State: ignore wl_seat::WlSeat);
delegate_noop!(State: ignore xdg_toplevel::XdgToplevel);
delegate_noop!(State: ignore wl_data_device_manager::WlDataDeviceManager);
