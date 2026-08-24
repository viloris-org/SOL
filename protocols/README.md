# Protocols

Home for protocol definitions and schemas that are **not** vendored wholesale
from upstream:

- SOL-private / SOL-extension **Wayland protocols** (`.xml`, generated bindings).
- The **Compositor ↔ Shell typed D-Bus schema** (PRD §11), selected by
  [ADR-0006](../docs/decisions/0006-shell-ipc-deferred.md) but not yet landed.
- Custom protocol glue for services (IME, portal glue, …).

Standard Wayland protocols we consume but do not author come from
`wayland-protocols`, `wayland-protocols-wlr`, and Smithay. Dependency presence
does not imply that SOL advertises or semantically implements an interface.

## Status

**No SOL-owned stable protocol or IPC schema exists yet.** Selected standard
globals are registered through Smithay, but their maturity ranges from a
registered handler to a narrow headless integration test. The authoritative
per-interface record is the
[Wayland protocol matrix](../docs/status/wayland-protocol-matrix.md).

ADR-0006 settles D-Bus as the compositor↔Shell transport. The versioned schema,
compositor service, Shell proxy, reconnect behavior, and end-to-end tests remain
Phase 1 work. Decorations, capture, output management, session lock, and other
interfaces remain explicit matrix entries rather than being treated as
implicitly supported.

## Landing a protocol here

1. Add / vendor the `.xml`.
2. Add the scanner/generation step and pin the supported interface version.
3. Wire the generated glue into its owning crate (`compositor/`, `shell/`, or
   the relevant service).
4. Add it to the protocol matrix with semantic, negative-path, interop, and
   real-boundary evidence requirements.
5. Record any non-obvious decision as an ADR.
