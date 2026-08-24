# SOL Documentation

Documentation is organized by audience and lifecycle:

| Doc | Audience | What it is |
|---|---|---|
| [PRD](PRD.md) | Product / everybody | Product requirements: vision, principles, architecture, MVP, phases (§1–42) |
| [OS Platform Definition](os-platform.md) | OS / security / application engineering | Normative boot, `.app`, atomic permissions, accounts, materials, and shared-runtime contracts |
| [Shell Spatial and Live Activity Contract](shell-experience.md) | Product / Shell / application engineering | Dock, Launcher, global menu, window controls, right-side status, tray, and Live Capsule |
| [Contextual IME PRD](contextual-ime-prd.md) | Product / IME engineering | Proposal and staged requirements for local contextual candidate ranking over fcitx5 |
| [ROADMAP](ROADMAP.md) | Engineering / product | Execution view of the PRD phases split into milestones, deliverables, and acceptance points |
| [Architecture](architecture.md) | Engineering | How the logical layers map to the monorepo, and boundary rules |
| [SolKit getting started](solkit-getting-started.md) | Application developers | Start a renderer-independent native app from the SolKit starter template |
| [Decision log](decisions/README.md) | Engineering | Architecture Decision Records (ADR) and the status-bearing decision register (PRD §41) |
| Component READMEs | Engineering | Per-crate docs (`compositor/`, `sdk/*`, `services/*`, `apps/*`, `shell/`, `protocols/`, `packaging/arch/`, `tests/`) |

## Reading order

1. **OS Platform Definition** — the new operating-system boundary and hard contracts.
2. **PRD** — the broader "what and why."
3. **Architecture** — the layer-to-code map and boundary rules.
4. **ROADMAP** — the "what, in what order" engineering view mapped to code.
5. **Decision log** — the "why this trade-off" records and what is still open.
6. **Component READMEs** — the "current state" of a given crate.

The OS rebaseline supersedes older statements that call SOL only a desktop
platform or use pacman/AUR as the native installed-system package authority.
For those topics, `os-platform.md` and ADR-0019 through ADR-0026 are the source
of truth.

## Quick status

| Area | Status |
|---|---|
| Platform / compositor | Phase 0 and Phase 1 complete — the Smithay compositor runs a standalone Wayland session with desktop-core protocol integration |
| Shell | Phase 1 top-bar layer-shell surface complete; broader shell experience is Phase 4 |
| SolKit SDK | Phase 2 in progress — token, semantic-component, layout, lifecycle/command, graphics, and motion foundations are implemented; rendering, accessibility, and keyboard work remain |
| Services | Scaffolds — `sol-ime` has Phase 1 protocol/frontend seams; fcitx5 transport and candidate-window rendering remain pending |
| Apps | Scaffolds — Files/Terminal/Settings in Phase 3 |
| Protocols / packaging | Early — no stable protocol; PKGBUILDs follow milestone completion |
| OS boot / image | Foundations — UKI-aware deployment manifests and deterministic A/B trial policy are implemented; signed UEFI execution, redundant boot/recovery authorities, durable state, and graphics handoff remain Phase 7 work |
| Native app platform | Planned — `.app`, `sol-pkg`, sandbox enforcement, and major/revision/feature runtime compatibility are Phases 8–9 |
| Accounts | Planned — `sol-securityd`-coordinated account grants, system-owned metadata, encrypted credentials, and generation-fenced leases |
| Material | Token foundation implemented — compositor-backed adaptive glass and hardware QA remain |
| GTK/Qt compatibility | Architecture accepted — private bundled runtimes, optional official adapters, equal capability/security contracts |
| Shell spatial model | Accepted — bottom Dock, upper-left foreground menu, upper-right status/Live Capsule; native surfaces pending |

## Conventions

- **Language:** documentation is written in standard American English.
- **Status labels:** `Complete` / `In progress` / `Scaffold` / `Planned` map
  to the [ROADMAP](ROADMAP.md) phases.
- **Link style:** relative links between docs in this repo.
