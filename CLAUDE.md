# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run

```bash
# Check entire workspace
cargo check --workspace

# Build entire workspace
cargo build --workspace

# Run the compositor (on current Wayland/X11 session, winit backend)
cargo run -p sol-compositor

# Run the compositor headless (no GPU, for CI/tests)
cargo run -p sol-compositor -- --headless

# Test the compositor
cargo test -p sol-compositor --test sol_session

# Run all tests
cargo test --workspace
```

## Development Environments

### winit backend (default)
Development path for Phase 0/1. Runs a window on the current Wayland/X11 session, no DRM grab required. Build with default feature:
```bash
cargo build -p sol-compositor
```

### udev backend (real hardware)
Phase 1+ for TTY sessions with DRM/GBM/libinput. Requires `libdisplay-info < 0.3.0`:
```bash
cargo build -p sol-compositor --features udev
```

## Architecture (Big Picture)

SOL is a layered Wayland-native desktop platform built from the ground up:

```
Applications  (apps/)   sol-files, sol-terminal, sol-settings
─────────────
SolKit        (sdk/)    sol-ui, sol-app, sol-graphics, sol-animation, sol-system, sol-design
─────────────
Runtime       (services/) sol-settingsd, sol-notificationd, sol-portal, sol-ime
─────────────
Shell         (shell/)  sol-shell — top bar, dock, launcher, overview
─────────────
Compositor    (compositor/) sol-compositor — Smithay Wayland compositor
─────────────
Linux         Arch/systemd/kernel/etc.
```

### Compositor (`compositor/`)
Core is `src/state.rs` containing `SolState` which owns all Smithay protocol state. The `main.rs` is a thin winit/udev backend event loop. Key components:
- `state.rs`: `SolState` - protocol handlers and state
- `main.rs`: backend event loops (`run_winit`, `run_headless`)
- `window.rs`: `WindowManager` - window layout, hit-testing, focus
- `grabs.rs`: interactive move/resize grab handlers
- `outputs.rs`: output management (add/remove outputs, HiDPI scale)

### Shell (`shell/`)
Desktop shell (top bar, dock, launcher). Runs as a separate process from the compositor for crash safety. Communicates via layer-shell protocol and D-Bus IPC (ADR-0006). Currently a scaffold/placeholder.

### SDK Crates (`sdk/`)
- `sol-design`: Design tokens (color, typography, spacing, motion) - single source of truth
- `sol-ui`: Semantic UI components (Button, TextField, etc.) - built on Slint
- `sol-app`: Application framework (lifecycle, commands, IPC)
- `sol-graphics`: Rendering abstraction
- `sol-animation`: Animation engine (easing, spring, interruption)
- `sol-system`: Restricted system API for apps

### Services (`services/`)
- `sol-settingsd`: System settings daemon
- `sol-notificationd`: Notification service
- `sol-portal`: Portal implementation
- `sol-ime`: IME frontend (fcitx5 engine bridge in Phase 1)

## Key Architectural Principles

1. **Compositor state ownership**: `SolState` owns all protocol state. Backends only drive it.
2. **Shell crash safety**: Shell is a separate process; compositor must not die if shell crashes.
3. **Consistency via tokens**: Visual parameters live in `sol-design`; components use tokens only.
4. **No XWayland**: Wayland-native only (PRD §4.2).
5. **Framework first**: Behavior comes from SolKit, not per-app conventions.

## Testing

The integration test (`compositor/tests/sol_session.rs`) validates end-to-end Wayland protocol round-trips:
- Spawns compositor in headless mode
- Waits for socket
- Runs test client against it
- Asserts successful toplevel configure round-trip

## Common Tasks

### Start compositor with custom socket
```bash
SOL_WAYLAND_SOCKET=my-sol WAYLAND_DISPLAY=wayland-sol cargo run -p sol-compositor -- --spawn weston-terminal
```

### Run a Wayland client against the compositor
```bash
WAYLAND_DISPLAY=wayland-sol weston-terminal
```

### Run formatter
```bash
cargo fmt --all
```

### Run clippy
```bash
cargo clippy --workspace -- -D warnings
```

## Documentation

- [README.md](README.md) - Project overview and status
- [docs/architecture.md](docs/architecture.md) - Detailed architecture mapping
- [docs/ROADMAP.md](docs/ROADMAP.md) - Phase-by-phase engineering plans
- [docs/decisions/README.md](docs/decisions/README.md) - ADRs for key design decisions
- [compositor/README.md](compositor/README.md) - Compositor-specific docs