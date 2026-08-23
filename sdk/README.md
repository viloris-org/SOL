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
├── SolCompat     toolkit adapter and portal contracts
└── SolTesting
```

## Permission tiers (PRD §23)

| Tier | Who uses it |
|---|---|
| Public | SolUI / SolApp / SolDocuments / SolGraphics / SolCompat contracts — available to third-party devs |
| Restricted | SolSystem / SolSecurityKit / future SolAccounts — typed requests touching permissions or system-managed identities |
| Private | SolShellKit — Shell-internal only |

Goal: third-party developers can reach near-first-party app quality rather
than being handicapped by an artificial first-party API advantage.

Installed apps bind through a signed runtime requirement consisting of a
`sol-runtime-N` major, minimum contract revision, and required stable feature
names. SolKit source bindings may evolve independently, but packaging must emit
that lower-level requirement; a runtime-major name alone is not sufficient.

## Status

- `sol-design`: Phase 0 seed (token seeds + consistency tests)
- All other crates: Phase 0 scaffolds; Phase 2 designs + dogfoods them

## Consistency mechanism (PRD §19.1)

- Single source of truth for visual parameters = `sol-design`.
- Apps write "what it is", not "how it looks".
- Design Review iron rule: bare hex / ms / f32 visual parameters are rejected.
- `sol-files` is the dogfooding baseline.
- golden-snapshot CI verifies consistency.

## Non-native toolkit adapters

Planned `sol-gtk` and `sol-qt` adapters are bundled with the app version that
matches its private toolkit runtime. They map supported toolkit APIs to stable
SOL portals, accounts, accessibility, lifecycle, appearance, and semantic
material contracts. SOL does not inject a host theme engine, Qt platform
plugin, or preload library into unrelated applications. See ADR-0024.

## See also

- [PRD §17 SolKit](../docs/PRD.md#17-solkit)
- [PRD §19 Design Tokens](../docs/PRD.md#19-design-tokens)
- [PRD §23 SDK permission tiers](../docs/PRD.md#23-sdk-permission-tiers)
- [Roadmap →](../docs/ROADMAP.md)
