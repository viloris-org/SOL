//! Shell connection to the compositor.
//!
//! Connects to the compositor's `wayland-sol` socket via `WAYLAND_DISPLAY`,
//! collects the globals we need from the registry (`wl_compositor`, `wl_shm`,
//! `zwlr_layer_shell_v1`), and exposes the handles the top bar needs to create
//! its layer surface. The shell is a **separate process** from `sol-compositor`
//! (PRD §11) — a crash here never takes down the compositor.

use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle,
    protocol::{wl_compositor::WlCompositor, wl_registry, wl_shm::WlShm},
};

use crate::Shell;
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::ZwlrLayerShellV1;

/// The globals a shell needs, bound from the compositor registry.
#[derive(Clone)]
pub struct Globals {
    pub compositor: WlCompositor,
    pub shm: WlShm,
    pub layer_shell: ZwlrLayerShellV1,
}

/// State driven while binding globals during the initial registry round-trip.
struct Handshake {
    compositor: Option<WlCompositor>,
    shm: Option<WlShm>,
    layer_shell: Option<ZwlrLayerShellV1>,
}

impl Handshake {
    fn ready(&self) -> bool {
        self.compositor.is_some() && self.shm.is_some() && self.layer_shell.is_some()
    }
    fn into_globals(self) -> Option<Globals> {
        Some(Globals {
            compositor: self.compositor?,
            shm: self.shm?,
            layer_shell: self.layer_shell?,
        })
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for Handshake {
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
            match interface.as_str() {
                "wl_compositor" if state.compositor.is_none() => {
                    state.compositor = Some(registry.bind::<WlCompositor, _, _>(name, 4, qh, ()));
                }
                "wl_shm" if state.shm.is_none() => {
                    state.shm = Some(registry.bind::<WlShm, _, _>(name, 1, qh, ()));
                }
                "zwlr_layer_shell_v1" if state.layer_shell.is_none() => {
                    state.layer_shell =
                        Some(registry.bind::<ZwlrLayerShellV1, _, _>(name, 4, qh, ()));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<WlCompositor, ()> for Handshake {
    fn event(
        _: &mut Self,
        _: &WlCompositor,
        _: <WlCompositor as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<WlShm, ()> for Handshake {
    fn event(
        _: &mut Self,
        _: &WlShm,
        _: <WlShm as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<ZwlrLayerShellV1, ()> for Handshake {
    fn event(
        _: &mut Self,
        _: &ZwlrLayerShellV1,
        _: <ZwlrLayerShellV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

/// The shell's live connection to the compositor.
pub struct ShellClient {
    pub qh: QueueHandle<Shell>,
    pub globals: Globals,
    event_queue: EventQueue<Shell>,
}

impl ShellClient {
    /// Connect, enumerate globals, and return a ready client.
    pub fn connect() -> Result<Self, Box<dyn std::error::Error>> {
        let conn = Connection::connect_to_env()?;

        // 1. Round-trip the registry with the handshake state.
        let mut hs_queue = conn.new_event_queue();
        let hs_qh = hs_queue.handle();
        conn.display().get_registry(&hs_qh, ());
        let mut hs = Handshake {
            compositor: None,
            shm: None,
            layer_shell: None,
        };
        for _ in 0..5 {
            hs_queue.blocking_dispatch(&mut hs)?;
            if hs.ready() {
                break;
            }
        }
        let globals = hs.into_globals().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "compositor is missing a required global (compositor/shm/layer-shell)",
            )
        })?;

        // 2. Build the live state + queue (referenced as `Shell` internally).
        let event_queue = conn.new_event_queue();
        let qh: QueueHandle<Shell> = event_queue.handle();

        Ok(ShellClient {
            qh,
            globals,
            event_queue,
        })
    }

    /// Pump the Wayland event queue (blocking until an event arrives).
    pub fn pump(&mut self, shell: &mut Shell) -> Result<(), Box<dyn std::error::Error>> {
        self.event_queue.blocking_dispatch(shell)?;
        Ok(())
    }
}
