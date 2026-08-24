# sol-compositor

SOL's Wayland compositor, built on [Smithay].

## Status

**Phase 0 (Foundation) milestone complete.** This is a functional Smithay
compositor that:

- starts a standalone Wayland session on a fixed socket (`wayland-sol` by
  default),
- serves standard Wayland clients (`wl_compositor`, `wl_shm`, `xdg_shell`,
  seat, data-device),
- follows keyboard focus for data-device selection offers,
- renders client surfaces with the GL renderer and drives frame callbacks,
- is integration-tested with real toplevel, layer-shell, and clipboard
  protocol round-trips.

The `--tty-udev` backend now owns a libseat/logind session, acquires DRM
devices, renders through GBM/EGL, submits KMS page flips, consumes libinput
keyboard/pointer events, switches VTs, and pauses/reacquires devices with the
session. Real-hardware validation still has to be performed from a local VT.

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

The tests boot the compositor headlessly, wait for its socket, and assert the
toplevel configure, Shell layer-surface, and UTF-8 data-device clipboard
round-trips succeed. They do not touch the user's live clipboard or prove
drag-and-drop behavior.

## Environment variables

| Variable | Meaning | Default |
|---|---|---|
| `SOL_WAYLAND_SOCKET` | Compositor listener socket name | `wayland-sol` |

## Architecture

- `src/state.rs` — `SolState`: the Smithay protocol state plus the `BufferHandler`,
  `CompositorHandler`, `ShmHandler`, `XdgShellHandler`, `SeatHandler`,
  `DataDeviceHandler` implementations.
- `src/main.rs` — winit/headless entry points and shared client helpers.
- `src/udev_runtime.rs` — libseat/libinput session lifecycle and the
  DRM/GBM/EGL/KMS event loop selected by `--tty-udev`.
- `examples/test-client.rs` — the reference Wayland client used by the test.
- `examples/clipboard-client.rs` — isolated data-source/data-offer transfer
  fixture.
- `tests/sol_session.rs` — the end-to-end session test.

The `udev` Cargo feature toggles the DRM/GBM/libinput/libseat backends for the
real TTY session (Phase 1+). It is not enabled by default because the winit
backend is the non-disruptive development path.

## Features

| Feature | Pulls in | Use |
|---|---|---|
| `default` = `winit` + `egl` | `smithay/backend_winit`, `backend_egl`, `renderer_gl`, `wayland_frontend` | Dev/CI: window on the current session |
| `udev` | DRM, GBM/EGL, libinput, udev, libseat | Real hardware TTY session with KMS presentation and VT lifecycle |

Headless operation is selected at runtime with `--headless`; it is not a Cargo
feature.

[Smithay]: https://github.com/Smithay/smithay
