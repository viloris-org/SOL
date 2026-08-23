# ADR-0023: SOL fluid material system

- **Status:** Accepted (design contract; renderer implementation pending)
- **Date:** 2026-08-22
- **Target phase:** Phase 2 token foundation / Phase 4 and 9 rendering

## Context

SOL needs a recognizable system material with the depth, continuity, and
adaptive translucency associated with liquid-glass interfaces. Treating that as
“add blur and lower opacity” would harm legibility, create excessive GPU work,
leak visual information across security boundaries, and fragment styling as
each app picks its own values.

## Decision

SOL defines its own semantic fluid material system in `sol-design`. The roles
are `Content`, `Chrome`, `Panel`, `Floating`, `Control`, `Sidebar`, `Dock`, and
`Capsule`. Apps select a role; they cannot provide raw blur, tint, saturation,
highlight, shadow, grain, or refraction values.

The renderer/compositor resolves those tokens using the current theme,
backdrop, accessibility preferences, power state, GPU capability, and frame
budget. Backdrop sampling and distortion remain compositor-owned effects; an
app never gains screenshot or cross-window pixel access by requesting glass.

Rules:

- Content is solid by default. Glass is reserved for navigation, controls,
  structural separation, and transient elevation.
- Material weight communicates hierarchy. Larger surfaces are thicker; small
  direct controls are lighter. Floating tasks use stronger separation.
- Multiple light glass layers are not stacked. Nested glass consolidates into
  one backdrop group or the inner surface becomes solid.
- Foreground contrast is continuously resolved by the system. Vibrancy is a
  semantic foreground treatment, not arbitrary translucent gray text.
- Material entry/exit changes blur, edge response, and scale together from the
  current presentation state. Motion remains interruptible; momentum is used
  only when inherited from user input.
- Reduced transparency removes blur/refraction. High contrast produces solid
  surfaces with explicit boundaries. Reduced motion uses a short cross-fade or
  static state instead of spatial materialization.
- Unsupported GPUs, remote sessions, battery-saving mode, or frame pressure
  degrade refraction, grain, saturation, then blur while preserving hierarchy.

The initial renderer-neutral token implementation lands in `sol-design` with
solid accessibility fallbacks. Real backdrop groups, adaptive luminance,
refraction, security validation, and hardware performance gates are later
renderer work.

## Consequences

- The system gets one consistent material grammar across Shell, first-party,
  and third-party SolUI applications.
- Non-SolUI apps may integrate through a constrained Wayland/system-decoration
  contract but cannot sample arbitrary protected content.
- Golden tests must validate semantic roles and accessibility fallbacks;
  visual/performance tests must cover busy light/dark backdrops and nested
  surfaces.
- Material effects are optional rendering detail, never required to understand
  hierarchy or operate the interface.

## Acceptance gates

- Text/icon contrast passes the selected SOL accessibility standard across the
  adversarial backdrop corpus.
- Reduced-transparency and high-contrast modes contain no backdrop sampling or
  refraction.
- Nested surfaces never exceed the allowed backdrop-group depth.
- Material motion is interruptible and starts from its live presentation state.
- The compositor meets frame-time and power budgets on the minimum supported
  GPU; fallback changes no layout or interaction semantics.
- Requesting a material never grants capture access or exposes backdrop pixels
  to the client.

## Related

- [OS Platform Definition](../os-platform.md)
- PRD §14 (motion), §19 (design tokens), §35 (accessibility)
- ADR-0004 (SolUI rendering substrate)
- ADR-0021 (application security)
- ADR-0024 (non-native toolkit compatibility)
- ADR-0025 (Dock, Sidebar, and Capsule roles)
