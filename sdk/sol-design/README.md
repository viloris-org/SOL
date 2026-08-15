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
├── material/    surface hierarchy (Base/Panel/Floating → shadow/blur)
├── motion/      motion tiers (Fast/Panel/Window/Workspace → duration+curve)
└── shadows/     shadow specs
```

## Consistency tests

- `tests/tokens.rs` verifies: monotonic spacing, semantic colors in [0,1]
  range, progressive motion durations.
- Phase 2 adds a `golden-snapshot` CI check: rendered output may contain only
  token-table values.

## Status

**Phase 0 seed present.** `color.rs` / `motion.rs` / `spacing.rs` / `radius.rs`
/ `material.rs` / `typography.rs` provide placeholder-but-constrained tokens;
theme colors are not finished design work. `sol-files` (Phase 3) will be the
dogfooding baseline that polishes the token shape.
