# First-party Applications

SOL's first-party applications. They also carry **SolKit dogfooding**
responsibility (PRD §24):

> When several apps need the same system-level interaction, improve SolKit
> first — do not hand-roll a workaround in each app.

## Applications

| App | Path | Priority | Status |
|---|---|---|---|
| `sol-files` | `apps/sol-files/` | MVP — dogfood baseline | Phase 0 scaffold → Phase 3 |
| `sol-terminal` | `apps/sol-terminal/` | MVP | Phase 0 scaffold → Phase 3 |
| `sol-settings` | `apps/sol-settings/` | MVP | Phase 0 scaffold → Phase 3 |
| `sol-installer` | `apps/sol-installer/` | OS release gate | Live welcome surface; installation backend pending → Phase 7 |
| `sol-store` | — | After MVP | Phase 4+ |
| `sol-viewer` | — | After MVP | Phase 4+ |
| `sol-monitor` | — | After MVP | Phase 4+ |
| `hyperion` | `apps/hyperion/` (planned) | Flagship professional AI IDE | Phase 10 |
| `phoebe` | `apps/phoebe/` (planned) | Professional photo workflow, benchmarked against Lightroom | Phase 10 |
| `iapetus` | `apps/iapetus/` (planned) | Professional image editing, benchmarked against Photoshop | Phase 10 |

Hyperion, Phoebe, and Iapetus are post-MVP products. They depend on the stable
runtime, application sandbox, GPU/media stack, and professional-workload
foundations delivered by Phases 5–9; they are not part of the Phase 3 closure
gate.

## Development loop (PRD §24)

```text
SolKit
  ↓
First-party App
  ↓
Framework limitation discovered
  ↓
Improve SolKit
  ↓
All applications benefit
```

## Consistency rules

- All first-party apps build exclusively via `solui` components + `sol-design`
  tokens; no home-rolled visual parameters or interaction components.
- `sol-files`, as the most complex app, carries the polish baseline; new
  components mature there first, then sink back into `sol-ui`.

## See also

- [PRD §24 First-party applications](../docs/PRD.md#24-first-party-applications)
- [PRD §25 Files](../docs/PRD.md#25-files)
- [PRD §26 Settings](../docs/PRD.md#26-settings)
- [PRD §27 Terminal](../docs/PRD.md#27-terminal)
- [PRD §27A Hyperion](../docs/PRD.md#27a-hyperion)
- [PRD §27B Phoebe](../docs/PRD.md#27b-phoebe)
- [PRD §27C Iapetus](../docs/PRD.md#27c-iapetus)
- [Roadmap →](../docs/ROADMAP.md)
