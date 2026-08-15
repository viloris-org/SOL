# 6. Compositor ↔ Shell IPC: D-Bus for typed IPC transport

- **Status:** **Accepted** ✅ (Phase 1 M1)
- **Date:** 2026-08-15
- **Decision date:** 2026-08-15
- **Target phase:** Phase 1

## Context

PRD §9.1 and §11 require `sol-compositor` and `sol-shell` to be separate
processes joined by **typed IPC**. The Phase 0 scaffold does not yet have a
shell, so the IPC shape was underspecified — but the compositor must not paint
itself into a corner that blocks IPC later.

## Decision (structural)

`sol-compositor` exposes a library path (`State` + backend entry points in
`compositor/src/state.rs`) that `sol-shell` (and tests) can link against or
embed. In Phase 0 this is in-process (the winit binary links `state`). The
**shell IPC boundary is preserved structurally** by keeping all compositor
logic in the `SolState` type rather than inside `main.rs`.

## Decision (transport) — Phase 1

**D-Bus** (`zbus`) is the IPC transport for the compositor↔shell boundary.

### Why D-Bus

1. **Idiomatic on Linux**: D-Bus is the standard IPC mechanism on Linux
   desktops. Every existing SOL system service (`sol-settingsd`,
   `sol-portal`, `sol-notificationd`) will already speak D-Bus, so the shell
   can reuse tooling, code patterns, and the existing service model.
2. **Typed + code-generated**: `zbus` generates type-safe Rust bindings from
   an XML interface definition, giving us compile-time-checked arguments
   rather than hand-marshaled messages. This satisfies PRD §11's "typed IPC"
   requirement with zero runtime ambiguity.
3. **Introspection**: D-Bus introspection means the interface is discoverable
   at runtime, which helps debugging, CLI tooling, and future accessibility /
   automation bridges.
4. **Process isolation preserved**: A crash in `sol-shell` does **not** crash
   `sol-compositor` — they are separate processes connected over the session
   bus. The compositor exposes a `sol.compositor` well-known name; the shell
   acts as a client.
5. **Non-busy state only**: The shell↔compositor traffic is low-frequency
   (workspace switches, dock visibility, top-bar updates, overview requests).
   These are interactive but not per-frame events. The high-frequency
   per-frame paths — frame callbacks, gesture drag progress — are handled
   locally within the compositor process (via Smithay's calloop loop); only
   final state changes propagate over D-Bus. This eliminates the marshaling
   concern that motivated the "shared-memory ring" option.

### Rejected options

2. **Wayland protocol** custom `sol` interface — rejected: would require
   shipping a `.protocol` XML and scanner step, adds Wayland-specific coupling
   that makes the shell depend on a live compositor display connection (harder
   to test in isolation), and offers no benefit at the data rates the shell
   actually needs.
3. **Calloop-driven shared-memory ring** — rejected: bespoke protocol with no
   tooling, no introspection, and no integration with the existing service
   model. Unnecessary latency for state that is never per-frame.

## Guidance for Phase 1

The compositor exposes D-Bus methods/properties on the
`sol.compositor` bus name (e.g. `ListWorkspaces`, `SwitchWorkspace`,
`SetDockVisibility`, `SetTopBarState`). All `SolState` mutations remain
reachable through typed Rust methods on `SolState`; the D-Bus interface
becomes a thin adapter layer around those methods — **no compositor rewrite**.

The shell (`sol-shell`) links against the compositor library path for shared
types but runs as a separate process, connecting to the compositor over the
session D-Bus.