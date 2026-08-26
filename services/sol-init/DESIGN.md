# sol-init - SOL Session Manager

SOL's daemon management system. Manages user-space daemons (compositor, shell, system services) with dependency resolution, restart policies, and D-Bus activation.

## Key Differences from systemd

- **Not systemd-compatible**: Uses `.daemon` files (not `.service`), TOML format (not INI)
- **SOL-native**: Designed specifically for SOL's capability model
- **Session-focused**: Manages only SOL user-space, not system services
- **Simpler**: No complex systemd features, just what SOL needs

## Quick Start

```bash
# Build
cargo build -p sol-init

# Test
cargo test -p sol-init

# Run (development mode)
cargo run -p sol-init
```

## Architecture

```
systemd (system)
└── sol-session@.service
    └── sol-init
        ├── sol-compositor (core, boot)
        ├── sol-shell (core, boot) 
        ├── sol-settingsd (system, dbus)
        └── sol-notificationd (system, dbus)
```

## Daemon Definition Format

Location:
- System: `/usr/share/sol/daemons/*.daemon`
- User (Phase 2+): `~/.local/share/sol/daemons/*.daemon`

Example (`sol-compositor.daemon`):

```toml
[Daemon]
name = "sol-compositor"
exec = "/usr/bin/sol-compositor"
type = "core"
start_mode = "boot"
restart_policy = "always"
after = []
requires = []
capabilities = ["compositor.render", "compositor.input"]

[Environment]
WAYLAND_DISPLAY = "sol-0"

[Resources]
memory_limit = "512M"
cpu_share = 1024
```

### Fields

**type**: `core` (compositor/shell) | `system` (services) | `application` (3rd party, Phase 2+)  
**start_mode**: `boot` (auto-start) | `dbus` (on-demand) | `socket` (Phase 2+)  
**restart_policy**: `always` | `on-failure` | `never`  
**after**: Start after these daemons (dependency order)  
**requires**: Hard dependencies (fail if missing)

## Usage

### Start Session
```bash
sol-init
```

Loads all `.daemon` files, starts boot daemons in dependency order.

### D-Bus Activation
```bash
sol-init --activate sol-settingsd
```

Used by D-Bus service files (`/usr/share/dbus-1/services/org.sol.Settings.service`).

## Implementation

- `src/lib.rs` - Main `SolInit` struct
- `src/daemon.rs` - Definition parsing and topological sort
- `src/process.rs` - Process lifecycle (spawn/wait/restart)
- `src/main.rs` - CLI entry point
- `daemons/` - System daemon definitions

## Testing

```bash
cargo test -p sol-init
```

Tests dependency resolution (topological sort) with circular dependency detection.

## Phase 2+ Extensions

### User Daemons
Third-party apps can install to `~/.local/share/sol/daemons/`:

```toml
[Daemon]
name = "my-sync-daemon"
type = "application"
start_mode = "dbus"
capabilities = ["network.access", "storage.user_data"]

[Metadata]
app_id = "com.myapp.sync"
display_name = "My Sync Service"
publisher = "My Company"
```

### Capability Enforcement
- `core`/`system`: capabilities advisory (trusted)
- `application`: capabilities enforced at runtime, require app store review

### Resource Limits
Enforced via cgroups:

```toml
[Resources]
memory_limit = "256M"
cpu_share = 512
```

## Integration

**Compositor**: Always first daemon started  
**Shell**: Starts after compositor (`after = ["sol-compositor"]`)  
**D-Bus**: On-demand activation via `.service` files  
**Capability System (Phase 2+)**: Runtime verification for `application` type
