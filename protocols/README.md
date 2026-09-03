# Protocols

Home for SOL-owned protocol definitions and schemas:

- The native **SOL Compositor Protocol (SCP)** wire schema.
- The **Compositor ↔ Shell typed D-Bus schema** (PRD §11), selected by
  [ADR-0006](../docs/decisions/0006-shell-ipc-deferred.md) but not yet landed.
- Custom typed IPC for services such as IME and portal glue.

## Status

SCP v2 is defined by [`scp/v2/scp.proto`](scp/v2/scp.proto). The compositor
generates Rust bindings with a vendored `protoc`, translates its domain
messages at `scp::wire`, and sends binary Protobuf inside the four-byte
length-framed Unix transport. File descriptors remain out-of-band via
`SCM_RIGHTS`; process-local descriptor numbers never enter the Protobuf.

ADR-0006 settles D-Bus as the compositor↔Shell control transport. The
compositor service, Shell proxy, reconnect behavior, and end-to-end tests remain
Phase 1 work. Decorations, capture, output management, session
lock, and other interfaces must land as explicit SCP capabilities rather than
implicit compatibility.

## Landing a protocol here

1. Add the versioned SCP or typed IPC schema.
2. Add deterministic code generation and pin the supported interface version.
3. Wire the generated glue into its owning crate (`compositor/`, `shell/`, or
   the relevant service).
4. Add semantic, negative-path, interop, and real-boundary evidence.
5. Record any non-obvious decision as an ADR.
