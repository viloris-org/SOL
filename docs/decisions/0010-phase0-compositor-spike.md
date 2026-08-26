# Phase 0 spike: functional Smithay compositor

**Status:** Complete — a `sol-compositor` binary and an integration-tested
client round-trip now exist in the workspace.

## What was proven

The PRD §38 Phase 0 success criterion is now demonstrable:

> "Start a standalone SOL Wayland session and run standard Wayland
> applications."

We run the compositor as a `winit` backend window on the surrounding session,
bind a `wayland-sol` socket, and drive a real Wayland client through it. The
integration test (`compositor/tests/sol_session.rs`) asserts:

1. `sol-compositor` starts and binds a socket,
2. `test-client` (a standard `wayland-client` example) connects,
3. the client creates an `xdg_toplevel` and attaches a `wl_shm` buffer,
4. the compositor dispatches the configure and the client acks it —
   a full protocol round-trip through the dispatch + render + frame-callback
   path.

```
cargo test -p sol-compositor --test sol_session   # PASSES
```

## Architecture locked in for Phase 0/1

- **State/service core:** `SolState` (`compositor/src/state.rs`) owns the
  Smithay protocol state (`wl_compositor`, `wl_shm`, `xdg_shell`, seat,
  data-device) and the handlers that drive them. Backends drive this one state
  — the same core later powers the udev/DRM backend for real hardware.
- **Backend abstraction respected:** winit is one driver of `SolState`. A
  `udev` Cargo feature toggles the DRM/udev/libinput/libseat backends for the
  real TTY session (Phase 1+).
- **Renderer:** Smithay `GlesRenderer` from `examples/minimal`. A single "draw
  toplevels into a framebuffer, then dispatch/flush clients, then submit"
  loop.
- **Frame callbacks:** `send_frames_surface_tree` publishes frame callbacks so
  clients repaint — this is what makes animation/gesture work later.

## Environment notes

- The workspace's default `cargo test`/`cargo build` uses the **winit**
  backend, which needs a running display. In this dev container the compositor
  surfaces as a window on the surrounding Wayland session.
- `features = ["udev"]` (DRM/GBM/libinput/libseat) currently does **not**
  build out-of-the-box: the crates' `libdisplay-info-sys` requires system
  `libdisplay-info < 0.3.0`, while the host system packages `0.3.0`. See ADR-0005. This is
  deferred to the real-hardware session (Phase 1).

## Next steps

- Phase 1: window management (move/resize/focus), workspaces, output
  management, and the first shell surface (layer-shell top bar).
- The Smithay `anvil` example in the vendored source (`smithay-0.7.0/anvil/`)
  is the reference for real hit-testing, focus, and DRM/udev backend wiring.
