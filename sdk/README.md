# SolKit — SOL Application Framework

SolKit is not just a widget toolkit. It is SOL's **application development
framework**, the connector between system experience and the application
ecosystem (PRD §17).

## Structure

```text
SolKit
├── SolUI        semantic UI components (Public)
├── SolApp       application lifecycle / Commands / Documents (Public)
├── SolGraphics  rendering abstraction (Public)
├── SolAnimation unified animation engine (Public)
├── SolWindow    window abstraction
├── SolCommands  unified command system
├── SolDocuments document model
├── SolStorage   storage abstraction
├── SolSystem    system API (Restricted)
├── SolAccessibility
└── SolTesting
```

## Permission tiers (PRD §23)

| Tier | Who uses it |
|---|---|
| Public | SolUI / SolApp / SolDocuments / SolGraphics — available to third-party devs |
| Restricted | SolSystem / SolSecurityKit — those touching system permissions |
| Private | SolShellKit — Shell-internal only |

Goal: third-party developers can reach near-first-party app quality rather
than being handicapped by an artificial first-party API advantage.

## Status

- `sol-design`: Phase 0 seed (token seeds + consistency tests)
- All other crates: Phase 0 scaffolds; Phase 2 designs + dogfoods them

## Consistency mechanism (PRD §19.1)

- Single source of truth for visual parameters = `sol-design`.
- Apps write "what it is", not "how it looks".
- Design Review iron rule: bare hex / ms / f32 visual parameters are rejected.
- `sol-files` is the dogfooding baseline.
- golden-snapshot CI verifies consistency.

## See also

- [PRD §17 SolKit](../../PRD.md#17-solkit)
- [PRD §19 Design Tokens](../../PRD.md#19-design-tokens)
- [PRD §23 SDK permission tiers](../../PRD.md#23-sdk-permission-tiers)
- [Roadmap →](../../ROADMAP.md)
