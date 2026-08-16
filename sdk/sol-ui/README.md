# sol-ui

SOL native UI components. SOL apps are written against a **semantic API**
(`Button(role: .Primary)`); visual properties (color / spacing / radius /
motion) come from `sol-design` tokens, never hand-written bare hex / ms /
f32. (PRD §18, §19.1 Consistency First.)

## Positioning

ADR-0004 lists Slint as a **candidate** rendering substrate — **undecided**.
A Phase 2 SolUI spike must validate layer-shell, fractional scaling, and
`sol-animation`'s interruptible / gesture-driven capabilities. Whichever way
that lands, the public `sol-ui` API layering stays the same:

```
SolKit Application API
        ↓
SolUI semantic components + Design Tokens + SolAnimation
        ↓
Slint (rendering / widget substrate — candidate)
```

App code never programs against `.slint` or Slint APIs directly.

## Responsibilities

- Layout (`HStack` / `VStack` semantic layout)
- Components (Button / Toolbar / Dialog / List / MenuItem / TextField / Tab …)
- Typography, theme, input, focus, animation (delegated to `sol-animation`)
- Accessibility (semantic tree, keyboard navigation, reduced motion)
- Rendering integration (decoupled from the underlying renderer, §19.1)

## Status

**Phase 2 foundation implemented.** Semantic components and `HStack` / `VStack`
layout are present. The renderer decision, keyboard/focus behavior, and
accessibility work remain in progress.

## Consistency iron rules (PRD §19.1)

- A `sol-ui` / first-party app commit **containing bare hex, bare ms, or bare
  f32 visual parameters is rejected at merge time**.
- Every new component must pass Design Review before entering `sol-ui`.
- `sol-files`, the most complex first app, carries the polish baseline.
- Consistency is verified by `golden-snapshot` CI: rendered output may contain
  only token values.
