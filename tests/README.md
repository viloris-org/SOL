# Integration tests

Cross-component, end-to-end tests live here. Component-internal tests stay in
each crate (`<crate>/tests/`).

## Status

No top-level harness exists yet. The first cross-component capability is
already proven in-crate: `compositor/tests/sol_session.rs` boots
`sol-compositor`, waits for its socket, and drives a real Wayland client
round-trip.

## Where the pieces live today

| Concern | Location | Notes |
|---|---|---|
| Compositor session round-trip | `compositor/tests/sol_session.rs` | starts compositor, runs `test-client`, asserts toplevel ack |
| Reference Wayland client | `compositor/examples/test-client.rs` | used by the session test and manual checks |
| Wayland clipboard round-trip | `compositor/examples/clipboard-client.rs` | isolated UTF-8 data-source/data-offer transfer; no live desktop clipboard |
| Design-token consistency | `sdk/sol-design/tests/tokens.rs` | monotonic spacing, color alpha range, motion durations |
| Future shell + compositor IPC | `tests/` | lands with Phase 1 typed IPC |

## Running

```bash
cargo test --workspace          # all component + integration tests
cargo test -p sol-compositor --test sol_session
```

> The compositor session suite runs the compositor with `--headless`, so it
> does not require a host display, GPU, or window manager.

## Roadmap

As things stabilize, `tests/` grows suites that combine multiple components:
compositor + shell over typed IPC, compositor + a SolKit app, services
(`notificationd` / `settingsd`) talking to the shell, and end-to-end IME.
See [docs/ROADMAP.md](../../docs/ROADMAP.md).
