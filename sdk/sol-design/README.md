# sol-design

SOL's **design-token** system: the single source of truth.

## Role

- The only crate allowed to define concrete visual parameters: colors,
  spacing, radii, durations, motion curves, font sizes, shadows, material
  layers.
- UI components and first-party apps **reference tokens by name**, never
  hand-writing bare hex / ms / f32. Type-safe wrapper types turn "wrong usage"
  into a **compile error** instead of style drift.
- Theme / skin switching touches only `sol-design`.

(PRD §19 Design Tokens / §19.1 Consistency First.)

## Token categories

```text
sol-design
├── color/       semantic colors (Surface/Elevated/Accent/Text/Border/Error…)
├── typography/  named sizes (Body/Title/Label/Display + weight)
├── spacing/     spacing scale (Xs/Sm/Md/Lg/Xl)
├── radius/      corner-radius scale (None/Sm/Md/Full)
├── material/    SOL fluid roles + accessibility-aware render specifications
├── motion/      motion tiers (Fast/Panel/Window/Workspace → duration+curve)
└── shadows/     shadow specs
```

## Consistency tests

- `tests/tokens.rs` verifies: monotonic spacing, semantic colors in [0,1]
  range, progressive motion durations.
- Phase 2 adds a `golden-snapshot` CI check: rendered output may contain only
  token-table values.

## Status

**Token foundation present.** `material.rs` defines `Content`, `Chrome`,
`Panel`, `Floating`, `Control`, `Sidebar`, `Dock`, and `Capsule` fluid-material
roles with solid reduced-transparency/high-contrast fallbacks. The values are
renderer-neutral design contracts; compositor-backed sampling, adaptive
luminance, refraction, and hardware performance validation remain future work.
Theme colors are not finished design work. `sol-files` remains the dogfooding
baseline.

Material rules come from [ADR-0023](../../docs/decisions/0023-sol-fluid-material.md):
dense content stays solid, glass expresses functional hierarchy, nested light
glass consolidates, and applications never receive backdrop pixels.
