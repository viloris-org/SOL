# sol-init

SOL session manager — manages SOL user-space daemons (compositor, shell, system services).

## Overview

`sol-init` is SOL's session-level init system. It replaces systemd user sessions with a simpler, SOL-native daemon manager that integrates deeply with SOL's capability model.

**Key differences from systemd:**
- Uses `.daemon` files, not `.service` files (avoid confusion)
- TOML format, not INI
- Focused on SOL daemons only, not general Linux services
- Integrated with SOL capability system (Phase 2+)
- D-Bus activation support

## Architecture

```
systemd (system PID 1)
└── sol-session@.service (wrapper)
    └── sol-init (SOL session manager)
        ├── sol-compositor (core, always first)
        ├── sol-audio (PipeWire, protected DSP scheduling)
        ├── sol-shell (core, after compositor)
        ├── sol-networkd (protected network service)
        ├── sol-portal (trusted authorization broker)
        ├── sol-settingsd (on-demand, D-Bus)
        └── sol-notificationd (on-demand, D-Bus)
```

## Daemon Definition Format

Daemons are defined in TOML `.daemon` files.

**System daemons:** `/usr/share/sol/daemons/`  
**User daemons (Phase 2+):** `~/.local/share/sol/daemons/`

Example: `sol-compositor.daemon`

```toml
[Daemon]
name = "sol-compositor"
exec = "/usr/bin/sol-compositor"
type = "core"  # core | system | application

start_mode = "boot"  # boot | dbus | socket
restart_policy = "always"  # always | on-failure | never

after = []  # Start after these daemons
requires = []  # Fail if these are missing

capabilities = [
    "compositor.render",
    "compositor.input",
    "system.drm"
]

[Environment]
WAYLAND_DISPLAY = "sol-0"

[Resources]  # Per-daemon overrides (pending)
memory_limit = "512M"
cpu_share = 1024
```

### Daemon Types

- **`core`**: Essential system components (compositor, shell) — trusted, no capability enforcement
- **`system`**: System services (settingsd, notificationd) — trusted, capabilities advisory
- **`application`**: Third-party background services (Phase 2+) — capabilities enforced

### Start Modes

- **`boot`**: Start automatically at session startup
- **`dbus`**: Start on-demand via D-Bus activation
- **`socket`**: Start on-demand via socket activation (Phase 2+)

### Restart Policies

- **`always`**: Restart on any exit (success or failure)
- **`on-failure`**: Restart only if exit code != 0
- **`never`**: Do not restart

## Usage

### Start sol-init
```bash
sol-init
```

Loads all `.daemon` files from `/usr/share/sol/daemons/`, starts `start_mode = "boot"` daemons in dependency order.

### D-Bus Activation
```bash
sol-init --activate sol-settingsd
```

Used by D-Bus service files like `/usr/share/dbus-1/services/org.sol.Settings.service`:

```ini
[D-BUS Service]
Name=org.sol.Settings
Exec=/usr/bin/sol-init --activate sol-settingsd
```

## Development

### Build
```bash
cargo build -p sol-init
```

### Test
```bash
cargo test -p sol-init
```

### Run (development mode, loads daemons from source tree)
```bash
DAEMON_DIR=services/sol-init/daemons cargo run -p sol-init
```

## Phase 2+ Extensions

### User Daemons
Third-party apps can install background services to `~/.local/share/sol/daemons/`.

Example: `~/.local/share/sol/daemons/my-sync-daemon.daemon`

```toml
[Daemon]
name = "my-sync-daemon"
exec = "/opt/my-app/bin/sync-daemon"
type = "application"

start_mode = "dbus"
restart_policy = "on-failure"

dbus_name = "com.myapp.SyncDaemon"

capabilities = [
    "network.access",
    "storage.user_data",
    "system.notifications"
]

[Metadata]
app_id = "com.myapp.sync"
display_name = "My App Sync Service"
publisher = "My Company"
```

**Capability enforcement:**
- `core`/`system` daemons: capabilities are advisory (trusted code)
- `application` daemons: capabilities are checked at runtime and must pass app store review

### Resource Scheduling

ADR-0029 Phase 1 creates fixed, trusted cgroup classes for the compositor,
audio, shell, network, system, foreground, background, and build workloads.
`sol-init` also applies class-specific nice, OOM, and I/O protection and scans
for build tools every 500 ms. Application daemon metadata cannot select an
elevated class.

Fine-grained legacy per-daemon overrides remain pending:

```toml
[Resources]
memory_limit = "512M"  # Enforced via cgroup memory controller
cpu_share = 1024       # Relative CPU weight
```

## Integration Points

### With compositor
- `sol-compositor.daemon` is always the first daemon started
- Other daemons can depend on it via `after = ["sol-compositor"]`

### With D-Bus
- Daemons with `start_mode = "dbus"` are activated on first D-Bus call
- D-Bus service files point to `sol-init --activate <name>`

### With shell
- `sol-shell.daemon` starts after compositor
- Can query running daemons via D-Bus (Phase 2+)

### With capability system (Phase 2+)
- Each daemon declares required capabilities
- sol-init verifies capabilities before starting `type = "application"` daemons
- Integrates with portal/permission manager

## Testing

Run unit tests:
```bash
cargo test -p sol-init
```

Test topological sort:
```bash
cargo test -p sol-init -- test_topological_sort
```

## Files

- `src/lib.rs` - Main `SolInit` struct and high-level logic
- `src/daemon.rs` - Daemon definition parsing and dependency resolution
- `src/process.rs` - Process lifecycle management (spawn/wait/restart)
- `../sol-scheduler/` - Trusted scheduling policy and PipeWire RT drop-in
- `src/main.rs` - CLI entry point
- `daemons/` - System daemon definitions (installed to `/usr/share/sol/daemons/`)
