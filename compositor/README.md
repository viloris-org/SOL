# sol-compositor

`sol-compositor` is the native SOL Compositor Protocol (SCP) service. It has no
Wayland, Smithay, wlroots, XWayland, winit, or X11 dependency in its build graph.

## Run

```bash
# Run with DRM/KMS backend (default - requires real hardware)
cargo run -p sol-compositor

# Run in headless mode for testing/CI
cargo run -p sol-compositor -- --headless

# Headless with frame dumps
SOL_SCP_FRAME_DUMP=/tmp/frame.png cargo run -p sol-compositor -- --headless

# Terminal 2: authenticated connect → surface → toplevel round trip
cargo run -p sol-compositor --example scp-client

# Shell layer-surface round trip
cargo run -p sol-shell -- --once
```

### Backend Modes

The compositor supports two presentation backends:

- **DRM/KMS (default)**: Scans out to real hardware displays via the Linux kernel's Direct Rendering Manager. Requires `/dev/dri/card0` access (typically the `video` group) and no other compositor running.

  The native backend also reads keyboard, mouse, touchpad, wheel, and direct-touch
  events from `/dev/input/event*`. The service user therefore needs input-device
  access (normally the `input` group or logind/seat ACLs). Devices are grabbed
  exclusively while the compositor owns DRM master and are rescanned for hot
  plug every two seconds. Set `SOL_INPUT_NO_GRAB=1` only for input debugging.

- **Headless (`--headless`)**: Software-only composition for testing and CI. Optionally dumps composed frames to PNG via `SOL_SCP_FRAME_DUMP`.

See [docs/drm-kms-implementation.md](../docs/drm-kms-implementation.md) for details on the hardware backend architecture.

The Unix socket defaults to `$XDG_RUNTIME_DIR/sol-compositor-0`. Set
`SOL_SCP_SOCKET` to a socket name or absolute path to override it. SCP frames
contain the binary Protobuf contract in
[`protocols/scp/v2/scp.proto`](../protocols/scp/v2/scp.proto) behind a four-byte
big-endian length prefix; buffer descriptors use `SCM_RIGHTS`.

Version-1 clients continue to start with `Connect`. Version-aware clients use
`ConnectVersioned { min_version, max_version }`; the current version is 2 and
adds sealed-FD screen capture, drag-action negotiation, and global shortcuts.

The transport caps concurrent connections at 256, disconnects peers that do not
authenticate within five seconds, bounds every outbound queue, and applies a
five-second write deadline. Shutdown is propagated to every client worker so
session-owned surfaces, tokens, shortcuts, and descriptors are reclaimed.

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

`CreateDmabufBuffer` imports one or more DMA-BUF descriptors with explicit
plane offsets, strides, format, and DRM modifier. The software compositor can
sample packed, single-plane linear buffers; the DRM backend exposes a GBM
explicit-modifier import path for tiled, compressed, or multi-plane GPU use.

## Test

```bash
cargo test -p sol-compositor
./scripts/validate-scp-only.sh
```

The active integration test is `tests/scp_session.rs`. Retired compositor
sources, clients, and compatibility fixtures live only in Git history; the
SCP-only guard prevents them or their dependencies from returning unnoticed.

## Active architecture

- `src/main.rs` — SCP service lifetime, backend selection, and presentation loop.
- `src/drm_backend.rs` — DRM/KMS backend for real hardware scanout.
- `src/native_input.rs` — Linux evdev discovery, hot plug, and SCP input dispatch.
- `src/scp/transport.rs` — Unix socket framing, peer credentials, and FD passing.
- `src/scp/state.rs` — authenticated sessions, capabilities, and object routing.
- `src/scp/compose.rs` — software composition of client surfaces into output framebuffers.
- `examples/scp-client.rs` — reference native client.
- `tests/scp_session.rs` — end-to-end native protocol/security checks.

The compositor is fully backend-agnostic: `ScpState` owns all protocol state, and
backends drive presentation by calling `present_frame()` to compose the desktop and
then scanning out the result. The DRM backend uses dumb buffers (CPU-accessible
scanout memory) and page flipping for tear-free updates.
