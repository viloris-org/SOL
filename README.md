# SOL

![status: concept/pre-alpha](https://img.shields.io/badge/status-concept%2Fpre--alpha-%23e9c46a) ![phase: 1-desktop-core](https://img.shields.io/badge/phase-1%20desktop%20core-%23a8dadc)

> A modern Linux desktop platform built on Arch Linux.

SOL is a **desktop platform, not a distribution**. It is architected and
engineered from the *platform layer* up — its own Wayland compositor, desktop
shell, application SDK (SolKit), design system, system services, and
first-party applications — while reusing proven Linux infrastructure (kernel,
systemd, PipeWire, NetworkManager, BlueZ, Mesa, polkit, udisks2).

> Don't reinvent Linux — redesign the Linux desktop.

## Status

**Concept / Pre-Alpha — Phase 0 (Foundation) ✅ complete, Phase 1 (Desktop Core) ✅ complete, Phase 2 (SolKit) in progress.**

The Phase 0 milestone ("start a standalone SOL Wayland session and run
standard Wayland applications") is **done**. Phase 1 M1 ("SOL can be used
as a basic daily-use Wayland compositor") is **done**: window management,
workspaces, multi-monitor, shell IPC, and IME are all implemented and
validated by integration tests.

Phase 2 M2 started: semantic components (Button, TextField, HStack, VStack)
and layout engine implemented. Slint/SolUI spike validation (ADR-0004) pending.

```bash
cargo test -p sol-compositor --test sol_session

# Live check: start the compositor, then point a standard app at it.
cargo run -p sol-compositor                       # terminal 1
WAYLAND_DISPLAY=wayland-sol weston-terminal        # terminal 2
```

**SolKit progress:** sol-ui now provides semantic component API (Button,
TextField, HStack, VStack) using sol-design tokens. Slint backend integration
pending spike validation.

## Repository layout

| Path | Purpose | Status |
|---|---|---|
| `compositor/` | `sol-compositor`: Smithay-based Wayland compositor | ✅ Phase 0+1 complete |
| `shell/` | `sol-shell`: top bar, dock, launcher, overview, system UI | ✅ Phase 1 shell top bar complete |
| `sdk/sol-design` | Design tokens (single source of truth for visuals) | ✅ token seeds + consistent tests |
| `sdk/sol-ui` | SolKit UI components (semantic, not visual-metrics) | ✅ Phase 2 buttons/layout/start |
| `sdk/sol-app` | Application framework (lifecycle, commands, …) | 🔲 placeholder → Phase 2 |
| `sdk/sol-graphics` | Rendering abstraction | 🔲 placeholder → Phase 2 |
| `sdk/sol-animation` | Animation engine (interruptible / motion tokens) | 🔲 placeholder → Phase 2 |
| `sdk/sol-system` | System API (restricted) | 🔲 placeholder → Phase 2 |
| `services/` | `sol-settingsd`, `sol-notificationd`, `sol-portal`, `sol-ime` | 🔲 scaffolds (Phase 1 IME ready) |
| `apps/` | First-party apps: Files, Terminal, Settings | 🔲 placeholders → Phase 3 |
| `protocols/` | Wayland protocol XML + IPC schemas | 🔲 no stable protocol yet |
| `packaging/arch/` | Pacman packaging for `[sol-core]`/`[sol-apps]`/`[sol-sdk]` | 🔲 early |
| `tests/` | Cross-component integration tests | ✅ Phase 0/1 integration tests |
| `docs/` | PRD, ROADMAP, engineering decisions | 🟡 living |

## Documentation

| Doc | What it is |
|---|---|
| [Product Requirements Document](docs/PRD.md) | What SOL is and why (§1–42); core principles, architecture, MVP, phases |
| [Roadmap](docs/ROADMAP.md) | Engineering execution view of the PRD phases, with deliverables & acceptance |
| [Decision log](docs/decisions/README.md) | ADRs (monorepo, workspace, Quickshell, Slint/SolUI, dev path, IPC, IME, distribution) |
| [Docs index](docs/README.md) | How the docs fit together + pointers |
| Component READMEs | `compositor/`, `sdk/*`, `services/*`, `apps/*`, `protocols/`, `packaging/arch/` |

## Build

```bash
# Whole workspace (Phase 0 defaults: winit + egl backends).
cargo check --workspace
cargo build --workspace

# Run the compositor (a window on your current Wayland/X11 session).
cargo run -p sol-compositor
```

The compositor binds a `wayland-sol` listener socket (override with
`SOL_WAYLAND_SOCKET`) and serves clients on it. The `udev` Cargo feature gates
the real-hardware DRM/GBM/libinput/libseat backends (Phase 1+; requires
compatible system `libdisplay-info`).

## Principles (from PRD §4)

- **Consistency First** — enforced by architecture, not discipline (§19.1).
- **Wayland Native** — no X11 session, no XWayland (§4.2).
- **Framework First** — behavior comes from SolKit, not from per-app
  conventions (§4.3).
- **Interactive Motion** — interruptible, gesture-driven, spring-based
  animation as part of the interaction model, not decoration (§4.4).
- **Linux Compatibility** — GTK/Qt/SDL/Flutter/Electron supported, Flatpak as
  a third-party runtime (§4.5).

## Also see

- [ROADMAP](docs/ROADMAP.md) — where SOL is going, phase by phase.
- [Compositor README](compositor/README.md) — how to run/extend the compositor.
