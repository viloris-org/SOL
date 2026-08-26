# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Product Philosophy

SOL is a **Linux Family OS** (like Android/Chrome OS), not a Linux distribution (like Ubuntu/Fedora):

- **No legacy compatibility** - We don't run arbitrary GTK/Qt/Wayland apps
- **Native-first** - Apps are built with SolKit SDK specifically for SOL
- **Curated ecosystem** - App store with reviewed apps, not a package manager
- **Platform security** - Capability model enforced by the OS, not optional
- **Atomic updates** - Image-based system, not package-by-package upgrades

This means: **No Wayland compatibility layer** (see ADR-0028). Third-party apps adapt through the `sol-runtime` SDK, similar to how Android apps use the Android SDK rather than raw Linux syscalls.

When working on SOL code, remember:
- We're building a new platform, not maintaining compatibility with existing Linux desktop apps
- Security and consistency come before ecosystem size
- The compositor speaks **only SCP** (SOL Compositor Protocol), not Wayland
- Apps that want to run on SOL must explicitly target it

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

SOL is a layered Linux Family platform built from the ground up:

```
Applications  (apps/)   sol-files, sol-terminal, sol-settings
─────────────
SolKit        (sdk/)    sol-ui, sol-app, sol-graphics, sol-animation, sol-system, sol-design
─────────────
Runtime       (services/) sol-settingsd, sol-notificationd, sol-portal, sol-ime
─────────────
Shell         (shell/)  sol-shell — top bar, dock, launcher, overview
─────────────
Compositor    (compositor/) sol-compositor — SCP (SOL Compositor Protocol) only
─────────────
Linux         Arch/systemd/kernel/etc.
```

### Compositor (`compositor/`)
Core is `src/state.rs` containing `SolState` which owns the SCP protocol state and window management. The `main.rs` is a thin winit/udev backend event loop. Key components:
- `state.rs`: `SolState` - SCP protocol handlers and state
- `main.rs`: backend event loops (`run_winit`, `run_headless`)
- `scp/`: SOL Compositor Protocol implementation (capability-based security)
- `window.rs`: `WindowManager` - window layout, hit-testing, focus
- `grabs.rs`: interactive move/resize grab handlers
- `outputs.rs`: output management (add/remove outputs, HiDPI scale)

**Protocol**: SOL uses SCP (SOL Compositor Protocol) exclusively. No Wayland compatibility (see ADR-0028).

### Shell (`shell/`)
Desktop shell (top bar, dock, launcher). Runs as a separate process from the compositor for crash safety. Communicates via SCP layer-shell capability and D-Bus IPC (ADR-0006). Currently a scaffold/placeholder.

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

1. **Compositor state ownership**: `SolState` owns all SCP protocol state. Backends only drive it.
2. **Shell crash safety**: Shell is a separate process; compositor must not die if shell crashes.
3. **Consistency via tokens**: Visual parameters live in `sol-design`; components use tokens only.
4. **No legacy compatibility**: No XWayland, no Wayland (ADR-0028). SCP only.
5. **Framework first**: Behavior comes from SolKit, not per-app conventions.
6. **Linux Family OS**: Like Android, apps target SOL explicitly through sol-runtime SDK.

## Testing

The integration test (`compositor/tests/sol_session.rs`) validates end-to-end SCP protocol round-trips:
- Spawns compositor in headless mode
- Waits for socket
- Runs SCP test client against it
- Asserts successful toplevel configure round-trip

## Common Tasks

### Start compositor in development mode
```bash
# Run in winit backend (window on host Wayland/X11 session)
cargo run -p sol-compositor

# Run headless (no GPU, for CI/tests)
cargo run -p sol-compositor -- --headless
```

### Test with SCP clients
```bash
# Run example SCP clients
cargo run -p sol-compositor --example scp-client
cargo run -p sol-compositor --example popup-client
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