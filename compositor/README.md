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

### Privileged identities in a development build

`sol-shell` and `sol-logind` gate layer shell and the session lock, so the
compositor will not hand either identity to a process just because
`/proc/<pid>/comm` says so — a process writes its own `comm`. The peer's
executable must also live in the trusted directory, `/usr/lib/sol` by default.

An uninstalled build therefore has to say where its binaries really are:

```bash
SOL_SCP_TRUSTED_BIN_DIR=$PWD/target/debug cargo run -p sol-compositor
```

Without it, `sol-shell` and `sol-logind` are refused at connect and the
compositor logs which directory it checked. Ordinary applications are
unaffected.

### Client buffers

Descriptors passed to `CreateShmPool` and `AttachBuffer` must be memfds sealed
with `F_SEAL_SHRINK` and at least as large as the geometry they describe. Both
are refused otherwise: an unsealed mapping can be truncated after the check, and
the SIGBUS that follows lands in the compositor, not in the client.

## Test

```bash
cargo test -p sol-compositor
./scripts/validate-scp-only.sh
```

The active integration test is `tests/scp_session.rs`. Retired compositor
sources, clients, and compatibility fixtures live only in Git history; the
SCP-only guard prevents them or their dependencies from returning unnoticed.

## Active architecture

- `src/main.rs` — SCP service lifetime and optional client spawning.
- `src/scp/transport.rs` — Unix socket framing, peer credentials, and FD passing.
- `src/scp/state.rs` — authenticated sessions, capabilities, and object routing.
- `examples/scp-client.rs` — reference native client.
- `tests/scp_session.rs` — end-to-end native protocol/security checks.

Renderer/input/DRM integration must be implemented against SCP-owned state;
there is no legacy frontend or compatibility fallback in the source tree.
