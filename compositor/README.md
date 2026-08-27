# sol-compositor

`sol-compositor` is the native SOL Compositor Protocol (SCP) service. It has no
Wayland, Smithay, wlroots, XWayland, winit, or X11 dependency in its build graph.

## Run

```bash
# Terminal 1
cargo run -p sol-compositor

# Terminal 2: authenticated connect → surface → toplevel round trip
cargo run -p sol-compositor --example scp-client

# Shell layer-surface round trip
cargo run -p sol-shell -- --once
```

The Unix socket defaults to `$XDG_RUNTIME_DIR/sol-compositor-0`. Set
`SOL_SCP_SOCKET` to a socket name or absolute path to override it. SCP frames
use a four-byte big-endian length prefix; buffer descriptors use `SCM_RIGHTS`.

## Test

```bash
cargo test -p sol-compositor
cargo tree --workspace --edges normal | rg -i 'wayland|smithay|wlroots'
```

The second command should print nothing. The active integration test is
`tests/scp_session.rs`; retired Wayland fixtures remain as migration reference
but are excluded from Cargo target discovery.

## Active architecture

- `src/main.rs` — SCP service lifetime and optional client spawning.
- `src/scp/transport.rs` — Unix socket framing, peer credentials, and FD passing.
- `src/scp/state.rs` — authenticated sessions, capabilities, and object routing.
- `examples/scp-client.rs` — reference native client.
- `tests/scp_session.rs` — end-to-end native protocol/security checks.

Renderer/input/DRM integration must be implemented against SCP-owned state;
the removed Wayland frontend is not a compatibility fallback.
