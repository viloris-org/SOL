# SOL Documentation

Documentation is organized by audience and lifecycle:

| Doc | Audience | What it is |
|---|---|---|
| [PRD](PRD.md) | Product / everybody | Product requirements: vision, principles, architecture, MVP, phases (§1–42) |
| [ROADMAP](ROADMAP.md) | Engineering / product | Execution view of the PRD phases split into milestones, deliverables, and acceptance points |
| [Architecture](architecture.md) | Engineering | How the logical layers map to the monorepo, and boundary rules |
| [Decision log](decisions/README.md) | Engineering | Architecture Decision Records (ADR) and the open-decision backlog (PRD §41) |
| Component READMEs | Engineering | Per-crate docs (`compositor/`, `sdk/*`, `services/*`, `apps/*`, `shell/`, `protocols/`, `packaging/arch/`, `tests/`) |

## Reading order

1. **PRD** — the "what and why." Start here.
2. **Architecture** — the layer-to-code map and boundary rules.
3. **ROADMAP** — the "what, in what order" engineering view mapped to code.
4. **Decision log** — the "why this trade-off" records and what is still open.
5. **Component READMEs** — the "current state" of a given crate.

## Quick status

| Area | Status |
|---|---|
| Platform / compositor | Phase 0 and Phase 1 complete — the Smithay compositor runs a standalone Wayland session with desktop-core protocol integration |
| Shell | Phase 1 top-bar layer-shell surface complete; broader shell experience is Phase 4 |
| SolKit SDK | Phase 2 in progress — token, semantic-component, layout, lifecycle/command, graphics, and motion foundations are implemented; rendering, accessibility, and keyboard work remain |
| Services | Scaffolds — `sol-ime` has Phase 1 protocol/frontend seams; fcitx5 transport and candidate-window rendering remain pending |
| Apps | Scaffolds — Files/Terminal/Settings in Phase 3 |
| Protocols / packaging | Early — no stable protocol; PKGBUILDs follow milestone completion |

## Conventions

- **Language:** documentation is written in standard American English.
- **Status labels:** `Complete` / `In progress` / `Scaffold` / `Planned` map
  to the [ROADMAP](ROADMAP.md) phases.
- **Link style:** relative links between docs in this repo.
