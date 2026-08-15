# 6. Compositor <-> Shell IPC: address the split, defer the wire format

- **Status:** Proposed (placeholder until Phase 1)
- **Date:** 2026-08-15
- **Target phase:** Phase 1

## Context

PRD §9.1 and §11 require `sol-compositor` and `sol-shell` to be separate
processes joined by **typed IPC**. The Phase 0 scaffold does not yet have a
shell, so the IPC shape is underspecified — but the compositor must not paint
itself into a corner that blocks IPC later.

## Decision (structural)

`sol-compositor` exposes a library path (`State` + backend entry points in
`compositor/src/state.rs`) that `sol-shell` (and tests) can link against or
embed. In Phase 0 this is in-process (the winit binary links `state`). The
**shell IPC boundary is preserved structurally** by keeping all compositor
logic in the `SolState` type rather than inside `main.rs`.

## Deferred (Phase 1)

The actual IPC transport, schema, and message set. Options under
consideration — not committed:

1. **D-Bus** (`zbus`-generated) — idiomatic on Linux, good introspection,
   integrates with the existing `sol-settingsd`/`sol-portal` service model.
   Risk: marshaling cost for high-frequency state (frame callbacks, gestures).
2. **Wayland protocol** custom `sol` interface — zero-copy for surface/
   animation state, natural fit for gesture/workspace progress messages.
   Risk: must ship a `.protocol` XML and scanner step in the build.
3. **Calloop-driven shared-memory ring** — lowest latency, matches the
   `Interactive Motion` PRD requirement (§154: "Finger movement → gesture
   progress → UI progress"). Risk: bespoke protocol, no tooling.

## Guidance for this spike

Phase 0 must **not** commit any of the above transports. The compositor should
keep all `SolState` mutations reachable through typed Rust methods so that
whichever IPC is chosen in Phase 1 only has to add an adapter layer around the
existing methods — no compositor rewrite.

Record the Phase 1 decision as a follow-up ADR when the shell starts needing
to drive workspaces, dock visibility, or top-bar updates from a process other
than the compositor.
