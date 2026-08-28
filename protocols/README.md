# Protocols

Home for SOL-owned protocol definitions and schemas:

- The native **SOL Compositor Protocol (SCP)** wire schema once stabilized.
- The **Compositor ↔ Shell typed D-Bus schema** (PRD §11), selected by
  [ADR-0006](../docs/decisions/0006-shell-ipc-deferred.md) but not yet landed.
- Custom typed IPC for services such as IME and portal glue.

## Status

**No SOL-owned stable external schema exists yet.** SCP is currently encoded by
Rust message types in `compositor/src/scp/protocol.rs` and exercised over the
native Unix transport. Publishing a language-neutral, versioned schema remains
follow-on work.

ADR-0006 settles D-Bus as the compositor↔Shell control transport. The versioned
schema, compositor service, Shell proxy, reconnect behavior, and end-to-end
tests remain Phase 1 work. Decorations, capture, output management, session
lock, and other interfaces must land as explicit SCP capabilities rather than
implicit compatibility.

## Landing a protocol here

1. Add the versioned SCP or typed IPC schema.
2. Add deterministic code generation and pin the supported interface version.
3. Wire the generated glue into its owning crate (`compositor/`, `shell/`, or
   the relevant service).
4. Add semantic, negative-path, interop, and real-boundary evidence.
5. Record any non-obvious decision as an ADR.
