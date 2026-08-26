# 5. Compositor dev path: winit-first, DRM/udev validated in Phase 1

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

## Historical evaluation

The full `smithay` crate set with the `udev`/`backend_drm` path also pulls
`libdisplay-info-sys`, which in this dev environment fails to build:

```
Package 'libdisplay-info' has version '0.3.0', required version is '< 0.3.0'
```

The host system ships `libdisplay-info` 0.3.0; the 0.2.x ABI pin is upstream-smithay
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

## Phase 1 update (2026-08-16)

`smithay-drm-extras` was an unused optional dependency, not part of SOL's
backend implementation. Its default `display-info` feature pulled
`libdisplay-info-sys` with the obsolete `< 0.3.0` pkg-config constraint.
Removing that unused dependency leaves the Smithay DRM/GBM/libinput/libseat
feature set intact and makes `cargo build -p sol-compositor --features udev`
build with current `libdisplay-info` 0.3.0.

The udev runtime now creates Smithay's real `UdevBackend`, obtains its initial
DRM-device list, reads connected connectors and modes from `/sys/class/drm`,
and creates matching Wayland output globals. Udev change events trigger a
fresh sysfs scan and reconcile adds, changes, and removals into the active
output set. The connector parser and reconciliation policy are tested from
filesystem fixtures; those tests deliberately validate the contract only and
are not presented as hardware validation.

The default layout is deterministic left-to-right placement using each
connector's first advertised mode. It is a backend default, not a replacement
for future persisted per-monitor configuration. The udev path is selected with
`--tty-udev` on a binary built using `--features udev`.

## Consequences

- `cargo check` / `cargo build` / `cargo test` for Phase 0 work on a plain
  dev box (or CI) with no root and no VT — the compositor renders into a
  window on the surrounding session.
- A `--headless` Cargo feature is reserved for future CI that runs the
  compositor socket-first (no window) and drives a headless client purely
  over the file descriptor — useful for environments with no display at all.
- The DRM/udev connector and hotplug path has a repeatable feature-build and
  fixture-test contract while the normal winit/headless paths remain CI-safe.
- A real-hardware smoke test still requires a local VT, a libseat/logind or
  seatd session, accessible `/dev/dri/card*`, and one or more connected DRM
  connectors. It has not been substituted by fixture tests.
- See `src/main.rs` (`run_winit`) and `compositor/tests/sol_session.rs` for
  the reference wiring.

## Hardware follow-up

Validate `--tty-udev` on Intel/AMD and NVIDIA GBM paths with one internal and
one external display; cover connector add/remove, mode changes, VT pause/resume
through libseat, and DRM/GBM presentation. These are hardware smoke tests and
remain distinct from the build and fixture verification above.
