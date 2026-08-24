# SOL

![status: concept/pre-alpha](https://img.shields.io/badge/status-concept%2Fpre--alpha-%23e9c46a) ![scope: operating-system](https://img.shields.io/badge/scope-operating%20system-%23a8dadc)

> A modern, application-first operating system built on the Linux kernel.

SOL is a **complete operating system**, not a desktop layer installed on an
arbitrary host distribution. It owns the boot experience, system image and
updates, package manager, application bundle format, atomic permission policy,
system-managed accounts, Wayland compositor, shell, system services,
application framework, and visual material language.

SOL still reuses proven Linux components where they are implementation
building blocks. Owning an OS boundary does not mean rewriting the kernel,
drivers, Mesa, PipeWire, or every protocol.

> Reuse Linux. Own the operating-system contract.

## Status

**Concept / Pre-Alpha — the desktop substrate exists; the OS foundation is now
the active product direction.**

The Phase 0 milestone ("start a standalone SOL Wayland session and run
standard Wayland applications") is **done**. Phase 1 M1 ("SOL can be used
as a basic daily-use Wayland compositor") is **done**: window management,
workspaces, multi-monitor, and shell IPC are implemented and covered by
integration tests. IME protocol integration and the frontend scaffold are
present; the fcitx5 transport remains a post-M1 follow-on.

Phase 2 M2 started: semantic components (Button, TextField, Toolbar, TabBar,
Tab, HStack, VStack) and layout engine implemented. Slint/SolUI spike
validation (ADR-0004) pending network access for dependency resolution.

The OS rebaseline adds these system foundations:

- `sol-image`: byte-reproducible, slot-bound deployment manifests with exact
  kernel, initrd, root-image, generation, and runtime-contract verification.
- `sol-boot`: redundant signed UEFI/recovery paths with trial activation and
  verified slot-bound A/B system deployments.
- `sol-pkg` + `sol-packaged`: one transactional manager for boot/recovery
  copies, system deployments, and signed `.app` bundles.
- `sol-securityd`: application identity, sandbox construction, capability
  grants, atomic consent/lease transactions, revocation, and audit.
- `sol-accountsd` + `sol-vaultd`: system-managed accounts and encrypted
  credentials exposed to apps only through explicit scoped handles.
- SOL Framework Runtime: major + minimum contract revision + named-feature
  descriptors, with compatible app resolution across OS rollback, so third-party
  apps vendor non-SOL dependencies without bundling the whole platform runtime.
- SOL Fluid Material: typed adaptive glass roles with compositor-owned effects
  and solid accessibility/performance fallbacks.
- `sol-gtk` / `sol-qt`: planned bundled adapters that give non-native toolkits
  the same portals, accounts, permissions, accessibility, and lifecycle APIs
  without injecting host libraries.

See [OS Platform Definition](docs/os-platform.md) for the normative boundary.

```bash
cargo test -p sol-compositor --test sol_session

# Live check: start the compositor, then point a standard app at it.
cargo run -p sol-compositor                       # terminal 1
WAYLAND_DISPLAY=wayland-sol weston-terminal        # terminal 2
```

**SolKit progress:** sol-ui provides semantic component API (Button,
TextField, Toolbar, TabBar, Tab, HStack, VStack) using sol-design
tokens. Slint backend integration pending spike validation.

## Repository layout

| Path | Purpose | Status |
|---|---|---|
| `compositor/` | `sol-compositor`: Smithay-based Wayland compositor | ✅ Phase 0+1 complete |
| `shell/` | `sol-shell`: top bar, dock, launcher, overview, system UI | ✅ Phase 1 shell top bar complete |
| `sdk/sol-design` | Design tokens (single source of truth for visuals) | ✅ token seeds + consistent tests |
| `sdk/sol-ui` | SolKit UI components (semantic, not visual-metrics) | ✅ Phase 2 buttons/layout/start |
| `sdk/sol-app` | Application framework (lifecycle, commands, …) | ✅ Phase 2 lifecycle + command foundations |
| `sdk/sol-graphics` | Rendering abstraction | ✅ Phase 2 abstraction foundations |
| `sdk/sol-animation` | Animation engine (interruptible / motion tokens) | ✅ Phase 2 semantic motion foundations |
| `sdk/sol-system` | System API (restricted) | 🔲 placeholder → Phase 2 |
| `services/` | `sol-settingsd`, `sol-notificationd`, `sol-portal`, `sol-ime` | 🟡 scaffolds; `sol-ime` has Phase 1 protocol/frontend seams, while fcitx5 transport and UI rendering remain pending |
| `apps/` | First-party apps: Files, Terminal, Settings | 🔲 placeholders → Phase 3 |
| `protocols/` | Wayland protocol XML + IPC schemas | 🔲 no stable protocol yet |
| `packaging/arch/` | Transitional Arch bootstrap/build packaging | 🟡 historical/transition |
| `boot/` | `sol-image` manifest tooling; target home of `sol-boot`, recovery, and verified-slot policy | 🟡 Phase 7 manifest foundation |
| `packaging/sol/` | Target home of `.app` tooling and `sol-pkg` contracts | 🔲 planned |
| `security/` | Target home of sandbox, permission, consent, and audit services | 🔲 planned |
| `accounts/` | Target home of system accounts, credential vault, and provider brokers | 🔲 planned |
| `compat/` | Target home of GTK/Qt adapters and generic Wayland compatibility contracts | 🔲 planned |
| `tests/` | Cross-component integration tests | ✅ Phase 0/1 integration tests |
| `docs/` | PRD, ROADMAP, engineering decisions | 🟡 living |

## Documentation

| Doc | What it is |
|---|---|
| [Product Requirements Document](docs/PRD.md) | What SOL is and why (§1–42); core principles, architecture, MVP, phases |
| [OS Platform Definition](docs/os-platform.md) | Normative OS boundary, `.app`, atomic permissions, accounts, materials, and runtime contracts |
| [Shell Experience](docs/shell-experience.md) | Dock, Launcher, global menu, window controls, status zone, tray, and Live Capsule contracts |
| [Roadmap](docs/ROADMAP.md) | Engineering execution view of the PRD phases, with deliverables & acceptance |
| [Decision log](docs/decisions/README.md) | ADRs for boot, packages, security, runtime, compositor, SDK, IPC, and distribution |
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
- **Transactional System** — boot, update, activation, rollback, and recovery
  are one coherent lifecycle.
- **Self-contained Applications** — each `.app` vendors its non-SOL
  dependencies and receives system access only through declared capabilities.
- **Explicit Minimum Authority** — each protected access is a smallest-scope,
  atomic, independently revocable grant; declaration and installation grant
  nothing.
- **Defined Authority Continuity** — verified same-publisher updates may retain
  durable grants but never live handles; uninstall/reinstall and discontinuous
  publishers inherit no authority.
- **System-managed Accounts** — applications receive scoped account handles,
  never ownership of the account database or durable credentials.
- **Stable Platform Runtime** — versioned SOL frameworks let applications
  avoid carrying common platform runtimes without depending on arbitrary host
  libraries.
- **Interactive Motion** — interruptible, gesture-driven, spring-based
  animation as part of the interaction model, not decoration (§4.4).
- **Linux Compatibility** — Wayland-native GTK/Qt/SDL/Flutter/Electron apps can
  be packaged as `.app`; compatibility does not weaken the SOL trust model.
- **Capability Equality** — native, integrated GTK/Qt, and generic Wayland apps
  share the same security and system-service guarantees; only visual fidelity
  differs by integration level.
- **Fluid Material** — adaptive translucent system chrome communicates depth,
  with solid reduced-transparency/high-contrast fallbacks.
- **Stable Shell Geography** — Dock at the bottom, foreground app menu at the
  upper-left, and trusted information/status/Live Capsule surfaces at the
  upper-right.

## Also see

- [ROADMAP](docs/ROADMAP.md) — where SOL is going, phase by phase.
- [Compositor README](compositor/README.md) — how to run/extend the compositor.

## License

SOL is licensed under the [BSD 3-Clause License](LICENSE).
