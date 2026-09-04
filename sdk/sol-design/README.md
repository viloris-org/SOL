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
├── radius/      corner-radius scale (None/Sm/Md/Lg/Xl/Full)
├── material/    Liquid Glass roles, optical specs, nesting + solid fallbacks
├── motion/      Fast/Panel/Material/Morph/Rebound/Window/Workspace springs
└── shadows/     shadow specs
```

## Consistency tests

- `tests/tokens.rs` verifies: monotonic spacing, semantic colors in [0,1]
  range, progressive motion durations.
- Phase 2 adds a `golden-snapshot` CI check: rendered output may contain only
  token-table values.

## Status

**Liquid Glass token foundation present.** Components select `Content`,
`Chrome`, `Panel`, `Floating`, `Control`, `Sidebar`, `Dock`, or `Capsule`.
`TokenMode` resolves theme, high-contrast, reduced-motion, text-scale, and
reduced-transparency variants. Nested glass consolidates into one backdrop
group instead of recursively blurring; unsupported rendering paths use the
same solid fallback without changing component layout or behavior.

The optical values are renderer-neutral contracts. Native compositor sampling,
adaptive luminance and hardware performance validation remain explicit work.
See [ADR-0023](../../docs/decisions/0023-sol-liquid-glass-material.md).
