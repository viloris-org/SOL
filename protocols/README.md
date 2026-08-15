# Protocols

Home for protocol definitions and schemas that are **not** vendored wholesale
from upstream:

- SOL-private / SOL-extension **Wayland protocols** (`.xml`, generated bindings).
- The **Compositor ↔ Shell typed IPC schema** (PRD §11) once the transport is
  chosen — see decision-backlog item #5 / [ADR-0006](../docs/decisions/0006-shell-ipc-deferred.md).
- Custom protocol glue for services (IME, portal glue, …).

Standard Wayland protocols we consume but do not author live in the
`wayland-protocols` / `smithay` dependencies, not here (e.g. `xdg-shell`,
`layer-shell`, `screencopy`).

## Status

**No stable protocol exists yet.** Phase 0 shipped the minimal standard
globals (`wl_compositor`, `wl_shm`, `xdg_shell`, seat, data-device) entirely
through Smithay's built-in handling — nothing custom was needed.

Decorations policy (server-side vs client-side), `layer-shell` integration
for the shell, screencopy for recording, and the IPC schema are all Phase 1
work (see [roadmap →](../../docs/ROADMAP.md)).

## Landing a protocol here

1. Add / vendor the `.xml`.
2. Add a build-time generation step (the tool depends on the chosen IPC
   transport).
3. Wire the generated glue into its owning crate (`compositor/`, `shell/`, or
   the relevant service).
4. Record any non-obvious decision as an ADR.
