# 5. Compositor dev path: winit-first, DRM deferred to Phase 1

- **Status:** Accepted
- **Date:** 2026-08-15
- **Target phase:** Phase 0 / Phase 1

## Context

Phase 0's success criterion is "start a standalone Wayland session and run a
standard Wayland application". PRD §38 also lists `Multi-monitor`,
`Suspend/Resume`, `NVIDIA`, touchpad, and hotplug as things that must not be
deferred past the end — but those are Phase 5 concerns, not Phase 0.

The workspace needs a compositor that (a) builds in CI / a dev container
without root or a VT grab, and (b) can later expand to real hardware
DRM/udev/libinput/libseat without a rewrite of the protocol core.

## Evaluation

The full `smithay` crate set with the `udev`/`backend_drm` path also pulls
`libdisplay-info-sys`, which in this dev environment fails to build:

```
Package 'libdisplay-info' has version '0.3.0', required version is '< 0.3.0'
```

Arch ships `libdisplay-info` 0.3.0; the 0.2.x ABI pin is upstream-smithay
specific. Pinning or patching would be brittle and not representative of the
real hardware target.

## Decision

1. The **default** feature set for `sol-compositor` is the `winit` + `egl`
   backends only: `smithay/backend_winit`, `smithay/renderer_gl`,
   `smithay/wayland_frontend`, and `smithay/backend_egl`.
2. Real-hardware backends (`backend_drm`, `backend_gbm`, `backend_libinput`,
   `backend_udev`, `backend_session_libseat`) are gated behind a `udev` Cargo
   feature, **not** the default. They are enabled at build time on the target
   hardware host (where `libdisplay-info` and the DRM stack are present).
3. The protocol core (`SolState`) is backend-agnostic: the winit driver and the
   eventual udev driver both call into the same `run_*` entry path against the
   same `SolState`. No compositor rendering or protocol logic is duplicated.

## Consequences

- `cargo check` / `cargo build` / `cargo test` for Phase 0 work on a plain
  Arch dev box (or CI) with no root and no VT — the compositor renders into a
  window on the surrounding session.
- A `--headless` Cargo feature is reserved for future CI that runs the
  compositor socket-first (no window) and drives a headless client purely
  over the file descriptor — useful for environments with no display at all.
- The DRM/udev path is not broken; it is simply opt-in. The build failure
  above is environment-specific to this container and does not block Phase 0.
- See `src/main.rs` (`run_winit`) and `compositor/tests/sol_session.rs` for
  the reference wiring.

## Open question

Whether Phase 1 needs `libdisplay-info < 0.3.0` patched in the AUR PKGBUILD or
whether the next `smithay` release bumps the `libdisplay-info-sys` pin — to be
settled when `features = ["udev"]` is exercised on target hardware (ADR-0006
will record the outcome).
