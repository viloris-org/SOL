# sol-logind

SOL's login screen — the authentication surface that decides who gets a session.

## Architecture

`sol-logind` is an SCP client, not a display server. It does not open KMS and it
does not run before the compositor. The compositor starts first; the greeter
connects to it and engages the **session lock**, then draws the login UI into a
lock surface.

```
Boot
  ↓
sol-init starts sol-compositor        (SCP socket appears)
  ↓
sol-logind connects → LockSession     (the desktop stops receiving input at once)
  ↓
login UI on the lock surface
  ↓
user authenticates (PAM)
  ↓ authorize UID while lock remains visible
sol-session --attach starts the desktop on the existing compositor
  ↓ shell commits every desktop surface
login surface crossfades to transparent → UnlockSession
  ↓
session ends → LockSession again
```

## Why the session lock

The lock is a capability of its own, reserved for this service by name and
granted to nothing else — not even `sol-shell` (ADR-0028,
`compositor/src/scp/capability.rs`). It is strictly stronger than layer shell:

- The surface sits above every layer, so the shell cannot cover or forge it.
- Keyboard focus is exclusive, so a keystroke cannot reach a window behind it.
- Screen capture is refused for the duration, by every client including this one.
- If this process dies the session stays **locked**, not open: the compositor
  *abandons* the lock rather than releasing it, and paints a blank fallback until
  a new greeter adopts it. A crash must never be a way back to the desktop.

## Where the password lives

`LoginUi` (`src/ui.rs`) owns the password. Keystrokes arrive as XKB keycodes over
SCP, are decoded by `src/scp/keys.rs`, and are applied to that state machine
directly — they never enter Slint's text-input stack, and the on-screen password
field only *displays* a string the service already masked or revealed.

Pointer input goes the other way, into Slint, because Slint is what knows where
the avatars and buttons ended up.

## Layout

```
src/
├── lib.rs        LoginService — ties the pieces together
├── main.rs       the connect → lock → render → authenticate loop
├── ui.rs         LoginUi state machine → renderer-neutral LoginFrame
├── render.rs     Slint software renderer on a custom, event-loop-less platform
├── auth.rs       PAM authentication and the session it keeps open
├── users.rs      account enumeration
├── session.rs    launching the authenticated user's desktop
└── scp/
    ├── lock.rs     the session-lock handshake, as a pure state machine
    ├── client.rs   the Unix socket, framing, and the SCM_RIGHTS buffer handoff
    ├── buffer.rs   the memfd the UI rasterizes into
    └── keys.rs     XKB keycode → character (US QWERTY)
```

## Status

- Presents over SCP as a session-lock client, with software-rendered frames
  attached as shared buffers and input routed back into the login state.
- Real PAM authentication, with the session held open for the desktop's lifetime.
- Nothing is visible on screen yet: the compositor has no renderer and no input
  backend (see `compositor/README.md`). The client half of both is complete and
  tested against the compositor's own state machine.

**Pending**: multi-output lock coverage beyond the primary display, decoding
against the keymap the compositor delivers (so non-US layouts can log in), rate
limiting on failed attempts, and the system actions (sleep, shut down) the design
calls for.

## Running

The compositor must be running first, and it must be willing to hand out the
session-lock capability. It grants a reserved identity only to a binary in its
trusted directory (`/usr/lib/sol` by default), so a development build has to say
where it lives:

```bash
# Terminal 1 — the compositor, told to trust the local build
SOL_SCP_TRUSTED_BIN_DIR=$PWD/target/debug cargo run -p sol-compositor

# Terminal 2 — the greeter (mock users, stub auth)
cargo run -p sol-logind -- --dev
```

Expect `session lock engaged` from the compositor and `session locked` from the
greeter. Nothing appears on screen, for the reason under **Status**.

In `--dev`, the launched session reuses the developer's `XDG_RUNTIME_DIR` and
attaches to the same compositor socket as the greeter. It never starts a second
compositor.

## Tests

```bash
cargo test -p sol-logind
```

`tests/scp_lock.rs` drives the lock handshake through a real `ScpState`
in-process, so the message sequence is checked against the compositor that will
answer it — including that an ordinary client is refused the lock, and that a
crashed greeter leaves the session locked.

## See also

- [Services README](../README.md)
- [ADR-0028: drop Wayland compatibility](../../docs/decisions/0028-drop-wayland-compatibility.md)
- [ADR-0027: SOL Compositor Protocol](../../docs/decisions/0027-sol-compositor-protocol.md)
- [Compositor session lock](../../compositor/src/scp/session_lock.rs)
