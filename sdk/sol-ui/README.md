# sol-ui

SOL native UI components. SOL apps are written against a **semantic API**
(`Button(role: .Primary)`); visual properties (color / spacing / radius /
motion) come from `sol-design` tokens, never hand-written bare hex / ms /
f32. (PRD §18, §19.1 Consistency First.)

## Positioning

ADR-0004 settles Slint as the native rendering/widget adapter. SolUI owns
retained semantic state and projects it into a private reactive Slint tree;
applications never see Slint types. The Phase 2 spike has repeatable headless
coverage for token projection, fractional-scale conversion, and external
`sol-animation` gesture progress. GPU performance, accessibility, real
multi-output, and distribution licensing remain explicit follow-ups in the
ADR.

```
SolKit Application API
        ↓
SolUI semantic components + Design Tokens + SolAnimation
        ↓
Slint (private rendering / widget adapter)
```

App code never programs against `.slint` or Slint APIs directly.

## Responsibilities

- Layout (`HStack` / `VStack` semantic layout)
- Components (Button / Toolbar / Dialog / List / MenuItem / TextField / Tab …)
- Typography, theme, input, focus, animation (delegated to `sol-animation`)
- Accessibility (semantic tree, keyboard navigation, reduced motion)
- Rendering integration (decoupled from the underlying renderer, §19.1)

## Status

**Phase 2 architecture spike complete.** `native` compiles the Wayland/winit
Slint adapter; the default feature set keeps deterministic semantic tests
headless and renderer-independent. Keyboard/focus behavior and accessibility
work remain in progress.

## Consistency iron rules (PRD §19.1)

- A `sol-ui` / first-party app commit **containing bare hex, bare ms, or bare
  f32 visual parameters is rejected at merge time**.
- Every new component must pass Design Review before entering `sol-ui`.
- `sol-files`, the most complex first app, carries the polish baseline.
- Consistency is verified by `golden-snapshot` CI: rendered output may contain
  only token values.
