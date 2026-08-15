# sol-compositor

SOL's Wayland compositor, built on [Smithay].

## Status

**Phase 0 (Foundation) milestone complete.** This is a functional Smithay
compositor that:

- starts a standalone Wayland session on a fixed socket (`wayland-sol` by
  default),
- serves standard Wayland clients (`wl_compositor`, `wl_shm`, `xdg_shell`,
  seat, data-device),
- renders client surfaces with the GL renderer and drives frame callbacks,
- is integration-tested with a real client round-trip.

Window management (move/resize/focus, workspaces), the layer-shell top bar,
XWayland, and the real DRM/udev session are **Phase 1** work. This crate
deliberately stays at "minimal well-formed compositor" until those land.

## Build & run

```bash
# Build only the Phase 0 default (winit + egl backends).
cargo build -p sol-compositor

# Run as a window on your current Wayland/X11 session.
cargo run -p sol-compositor
```

### Point a client at it (separate terminal)

```bash
# Use the built-in test client...
WAYLAND_DISPLAY=wayland-sol cargo run -p sol-compositor --example test-client

# ...or any standard Wayland app.
WAYLAND_DISPLAY=wayland-sol weston-terminal
```

### Test

```bash
cargo test -p sol-compositor --test sol_session
```

The test boots the compositor, waits for its socket, runs the example client,
and asserts the toplevel configure round-trip succeeds.

## Environment variables

| Variable | Meaning | Default |
|---|---|---|
| `SOL_WAYLAND_SOCKET` | Compositor listener socket name | `wayland-sol` |

## Architecture

- `src/state.rs` — `SolState`: the Smithay protocol state plus the `BufferHandler`,
  `CompositorHandler`, `ShmHandler`, `XdgShellHandler`, `SeatHandler`,
  `DataDeviceHandler` implementations.
- `src/main.rs` — the `winit` backend event loop: render toplevel surfaces,
  dispatch/flush client events, publish frame callbacks, accept new clients.
- `examples/test-client.rs` — the reference Wayland client used by the test.
- `tests/sol_session.rs` — the end-to-end session test.

The `udev` Cargo feature toggles the DRM/GBM/libinput/libseat backends for the
real TTY session (Phase 1+). It is not enabled by default because the current
dev environment's `libdisplay-info` (0.3.0) is too new for the upgraded
`libdisplay-info-sys` pin — see the repo decision log ADR-0005.

## Features

| Feature | Pulls in | Use |
|---|---|---|
| `default` = `winit` + `egl` | `smithay/backend_winit`, `backend_egl`, `renderer_gl`, `wayland_frontend` | Dev/CI: window on the current session |
| `udev` | `smithay/backend_drm`, `backend_gbm`, `backend_libinput`, `backend_udev`, `backend_session_libseat` | Real hardware TTY session (Phase 1) |
| `headless` | — | Reserved for socket-first CI (no window) |

[Smithay]: https://github.com/Smithay/smithay
