# 2. Rust workspace layout

- Status: Accepted
- Date: 2026-08-15

## Context

The repository needs a workspace layout that maps to PRD sections 7, 10, and 39.

## Proposed layout

- Binaries: `compositor`, `shell`, `services/*`, `apps/*`
- Libraries: `sdk/*` (`sol-ui`, `sol-app`, `sol-graphics`, `sol-animation`, `sol-system`)
- Non-Cargo areas: `protocols`, `packaging/sol`, `tests`, `docs`

## Open questions

- Whether `sdk` needs a meta crate (`solkit`) re-exporting the public SDK crates.
- Whether `services` should be libraries plus thin daemon binaries.

## Status

Open for confirmation before the first Smithay spike.
