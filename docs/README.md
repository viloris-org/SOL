# SOL Documentation

Documentation is organized by audience and lifecycle:

| Doc | Audience | What it is |
|---|---|---|
| [PRD](PRD.md) | Product / everybody | Product requirements: vision, principles, architecture, MVP, phases (§1–42) |
| [OS Platform Definition](os-platform.md) | OS / security / application engineering | Normative boot, `.app`, atomic permissions, accounts, materials, and shared-runtime contracts |
| [Shell Spatial and Live Activity Contract](shell-experience.md) | Product / Shell / application engineering | Dock, Launcher, global menu, window controls, right-side status, tray, and Live Capsule |
| [Contextual IME PRD](contextual-ime-prd.md) | Product / IME engineering | Proposal and staged requirements for local contextual candidate ranking over fcitx5 |
| [ROADMAP](ROADMAP.md) | Engineering / product | Execution view of the PRD phases split into milestones, deliverables, and acceptance points |
| [Historical Wayland protocol matrix](status/wayland-protocol-matrix.md) | Compositor / Shell engineering | Retired frontend evidence retained for migration history |
| [Architecture](architecture.md) | Engineering | How the logical layers map to the monorepo, and boundary rules |
| [SolKit getting started](solkit-getting-started.md) | Application developers | Start a renderer-independent native app from the SolKit starter template |
| [Decision log](decisions/README.md) | Engineering | Architecture Decision Records (ADR) and the status-bearing decision register (PRD §41) |
| Component READMEs | Engineering | Per-crate docs (`compositor/`, `sdk/*`, `services/*`, `apps/*`, `shell/`, `protocols/`, `packaging/sol/`, `tests/`) |

## Reading order

1. **OS Platform Definition** — the new operating-system boundary and hard contracts.
2. **PRD** — the broader "what and why."
3. **Architecture** — the layer-to-code map and boundary rules.
4. **ROADMAP** — the "what, in what order" engineering view mapped to code.
5. **Protocol docs** — current SCP status plus the historical frontend matrix.
6. **Decision log** — the "why this trade-off" records and what is still open.
7. **Component READMEs** — the "current state" of a given crate.

The OS rebaseline supersedes older statements that call SOL only a desktop
platform or use pacman/AUR as the native installed-system package authority.
For those topics, `os-platform.md` and ADR-0019 through ADR-0026 are the source
of truth.

## Quick status

| Area | Status |
|---|---|
| Platform / compositor | Native SCP transport/state work headlessly; rendering, input, output, and real DRM closure remain |
| Shell | Top-bar configure/commit slice exists; layer mapping/rendering and compositor D-Bus remain Phase 1 gates; broader experience is Phase 4 |
| SolKit SDK | Phase 2 in progress — token, semantic-component, layout, lifecycle/command, graphics, and motion foundations are implemented; rendering, accessibility, and keyboard work remain |
| Services | Mixed S1–S3 foundations — `sol-ime` has protocol/frontend seams and an fcitx5 transport slice; candidate rendering and end-to-end desktop input remain open |
| Apps | Scaffolds — Files/Terminal/Settings in Phase 3 |
| Protocols / packaging | Early — Rust SCP messages work internally; no stable language-neutral SCP schema yet |
| OS boot / image | Foundations — development manifests and a deterministic deployment selector exist; stable Stage-0, independent recovery, content/placement separation, authenticated health, anti-rollback, data barriers, and complete production UKIs remain Phase 7 work |
| Native app platform | Planned — `.app`, `sol-pkg`, sandbox enforcement, and major/revision/feature runtime compatibility are Phases 8–9 |
| Accounts | Planned — `sol-securityd`-coordinated account grants, system-owned metadata, encrypted credentials, and generation-fenced leases |
| Material | Token foundation implemented — compositor-backed adaptive glass and hardware QA remain |
| GTK/Qt adapters | Architecture accepted — private bundled runtimes, explicit SOL adapters, equal capability/security contracts |
| Shell spatial model | Accepted — bottom Dock, upper-left foreground menu, upper-right status/Live Capsule; native surfaces pending |

## Conventions

- **Language:** documentation is written in standard American English.
- **Status labels:** deliverables use S0 Planned through S5 Release-ready as
  defined by the [ROADMAP](ROADMAP.md); a phase is complete only when every
  required closure item is S5.
- **Link style:** relative links between docs in this repo.
