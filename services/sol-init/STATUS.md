## sol-init

**SOL session manager** — manages SOL user-space daemons with dependency resolution, restart policies, and D-Bus activation.

### Status: Phase 1 Implementation Complete

Core functionality implemented:
- ✅ `.daemon` file parsing (TOML format)
- ✅ Dependency resolution (topological sort with cycle detection)
- ✅ Process lifecycle management (spawn/wait/restart)
- ✅ Restart policies (always/on-failure/never)
- ✅ D-Bus activation support (`--activate` flag)
- ✅ Signal handling (graceful shutdown on SIGTERM/SIGINT)
- ✅ System daemon definitions (compositor, shell, settingsd, notificationd)

### Architecture

```
sol-init (1.3MB binary)
├── Load daemons from /usr/share/sol/daemons/*.daemon
├── Topologically sort by dependencies (after/requires)
├── Start boot daemons in order
└── Monitor processes, restart per policy
```

**Key Design Decisions:**
- Uses `.daemon` extension (not `.service`) to avoid systemd confusion
- TOML format (not INI) — cleaner, native Rust support
- Session-level only — not a system init replacement
- Capability declarations ready for Phase 2+ enforcement

### Example Daemon Definition

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
```

### Phase 2+ Extensions (Not Yet Implemented)

- [ ] User daemon directory (`~/.local/share/sol/daemons/`)
- [ ] Capability enforcement for `type = "application"` daemons
- [x] ADR-0029 Phase 1 cgroup hierarchy and trusted process placement
- [x] OOM/nice/I/O protection for compositor, shell, network, and system daemons
- [x] Automatic build-process containment in `sol-build`
- [ ] Per-daemon legacy resource overrides (`memory_limit`, `cpu_share`)
- [ ] Socket activation (in addition to D-Bus)
- [ ] IPC interface for querying daemon status

### Files

- `services/sol-init/src/` — Core implementation
  - `lib.rs` — Main `SolInit` struct
  - `daemon.rs` — Definition parsing and dependency resolution
  - `process.rs` — Process lifecycle management
  - `main.rs` — CLI entry point
- `services/sol-init/daemons/` — System daemon definitions
  - `sol-compositor.daemon`
  - `sol-shell.daemon`
  - `sol-settingsd.daemon` (D-Bus activated)
  - `sol-notificationd.daemon` (D-Bus activated)
  - `example-app-daemon.daemon` (Phase 2+ template)
- `services/sol-init/README.md` — User documentation
- `services/sol-init/DESIGN.md` — Design overview

### Testing

```bash
cargo test -p sol-init
cargo clippy -p sol-init -- -D warnings
```

All tests passing, clippy clean.
