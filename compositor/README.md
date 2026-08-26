# sol-compositor

SOL's compositor. The native frontend is SCP (SOL Compositor Protocol); the
Smithay/Wayland frontend remains temporarily while SCP rendering is brought up.

## Native SCP quick start

The compositor always opens `$XDG_RUNTIME_DIR/sol-compositor-0` (override the
name with `SOL_SCP_SOCKET`). Messages use a 4-byte big-endian length prefix;
buffer descriptors travel through `SCM_RIGHTS`.

```bash
# Terminal 1
cargo run -p sol-compositor -- --headless

# Terminal 2
cargo run -p sol-compositor --example scp-client
```

The example performs a real authenticated connect → surface → toplevel
round-trip. It does not simulate responses.

## Status

**Phase 1 reopened after an implementation/evidence audit.** The accepted
foundation spike and current implementation slices can:

- start a standalone Wayland session on a fixed socket (`wayland-sol` by
  default),
- advertise the current core globals (`wl_compositor`, `wl_shm`, `xdg_shell`,
  seat, output, data-device, fractional scale, layer-shell, text input, and
  input method),
- follow keyboard focus for data-device selection offers,
- render client surfaces with the GL renderer and drive frame callbacks,
- pass repository-owned headless toplevel, fractional-scale, layer-shell
  configure/commit, and clipboard round trips.

Those facts do not yet establish a basic daily-use compositor. Popup semantics,
complete xdg state/lifecycle, layer-surface mapping/rendering, DnD, xdg-output,
common compatibility protocols, compositor↔Shell D-Bus, end-to-end IME,
multi-output behavior, and representative external-client validation remain
open. See the [Roadmap] and [Wayland protocol matrix] for the closure gates.

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
round-trips succeed. They have no GPU or visible output; a committed Shell
buffer is not proof that a layer surface was mapped and rendered. They do not
touch the user's live clipboard or prove drag-and-drop behavior.

## Environment variables

| Variable | Meaning | Default |
|---|---|---|
| `SOL_SCP_SOCKET` | Native SCP socket name or absolute path | `sol-compositor-0` |
| `SOL_WAYLAND_SOCKET` | Compositor listener socket name | `wayland-sol` |

## Architecture

- `src/state.rs` — `SolState`: the Smithay protocol state plus the `BufferHandler`,
  `CompositorHandler`, `ShmHandler`, `XdgShellHandler`, `SeatHandler`,
  `DataDeviceHandler` implementations.
- `src/main.rs` — winit/headless entry points and shared client helpers.
- `src/scp/transport.rs` — native Unix socket, framing, peer credentials, and FD passing.
- `src/scp/state.rs` — authenticated sessions, capabilities, and SCP object routing.
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
[Roadmap]: ../docs/ROADMAP.md
[Wayland protocol matrix]: ../docs/status/wayland-protocol-matrix.md
