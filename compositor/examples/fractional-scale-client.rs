//! Minimal client that verifies SOL's wp_fractional_scale_v1 contract.

use std::time::{Duration, Instant};

use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop};
use wayland_client::protocol::{wl_compositor, wl_registry, wl_surface};
use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1, wp_fractional_scale_v1,
};

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
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Err(error) = queue.blocking_dispatch(&mut state) {
            eprintln!("dispatch error: {error}");
            return 2;
        }
        if let Some(scale) = state.preferred_scale {
            eprintln!("fractional scale preferred={scale}");
            return if scale == 150 { 0 } else { 3 };
        }
    }
    eprintln!("timeout waiting for fractional scale preference");
    4
}

#[derive(Default)]
struct State {
    surface: Option<wl_surface::WlSurface>,
    manager: Option<wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1>,
    scale: Option<wp_fractional_scale_v1::WpFractionalScaleV1>,
    preferred_scale: Option<u32>,
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
        let wl_registry::Event::Global { name, interface, version } = event else {
            return;
        };
        match interface.as_str() {
            "wl_compositor" => {
                let compositor = registry.bind::<wl_compositor::WlCompositor, _, _>(
                    name, version.min(6), qh, (),
                );
                state.surface = Some(compositor.create_surface(qh, ()));
                if let (Some(manager), Some(surface)) = (&state.manager, &state.surface) {
                    state.scale = Some(manager.get_fractional_scale(surface, qh, ()));
                    surface.commit();
                }
            }
            "wp_fractional_scale_manager_v1" => {
                let manager = registry.bind::<
                    wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
                    _,
                    _,
                >(name, version.min(1), qh, ());
                state.manager = Some(manager);
                if let (Some(manager), Some(surface)) = (&state.manager, &state.surface) {
                    state.scale = Some(manager.get_fractional_scale(surface, qh, ()));
                    surface.commit();
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wp_fractional_scale_v1::WpFractionalScaleV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &wp_fractional_scale_v1::WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event {
            state.preferred_scale = Some(scale);
        }
    }
}

delegate_noop!(State: ignore wl_compositor::WlCompositor);
delegate_noop!(State: ignore wl_surface::WlSurface);
delegate_noop!(State: ignore wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1);
