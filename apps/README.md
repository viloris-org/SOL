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
| `sol-store` | — | After MVP | Phase 4+ |
| `sol-viewer` | — | After MVP | Phase 4+ |
| `sol-monitor` | — | After MVP | Phase 4+ |

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

- [PRD §24 First-party applications](../PRD.md#24-first-party-applications)
- [PRD §25 Files](../PRD.md#25-files)
- [PRD §26 Settings](../PRD.md#26-settings)
- [PRD §27 Terminal](../PRD.md#27-terminal)
- [Roadmap →](../ROADMAP.md)
