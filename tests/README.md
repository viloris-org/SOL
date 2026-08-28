# Integration tests

Cross-component, end-to-end tests live here. Component-internal tests stay in
each crate (`<crate>/tests/`).

## Status

No top-level harness exists yet. The first cross-component capability is
already proven in-crate: `compositor/tests/scp_session.rs` boots
`sol-compositor`, waits for its socket, and drives authenticated native SCP
round-trips.

## Where the pieces live today

| Concern | Location | Notes |
|---|---|---|
| Compositor session round-trip | `compositor/tests/scp_session.rs` | starts compositor and validates native transport, identity, capabilities, and toplevel state |
| Reference SCP client | `compositor/examples/scp-client.rs` | authenticated surface/toplevel round-trip used for manual checks |
| SCP-only boundary | `scripts/validate-scp-only.sh` | rejects legacy dependencies, socket variables, and retired source paths |
| Design-token consistency | `sdk/sol-design/tests/tokens.rs` | monotonic spacing, color alpha range, motion durations |
| Future shell + compositor IPC | `tests/` | lands with Phase 1 typed IPC |

## Running

```bash
cargo test --workspace          # all component + integration tests
cargo test -p sol-compositor --test scp_session
```

> The compositor session suite is headless by construction, so it does not
> require a host display, GPU, or window manager.

## Roadmap

As things stabilize, `tests/` grows suites that combine multiple components:
compositor + shell over typed IPC, compositor + a SolKit app, services
(`notificationd` / `settingsd`) talking to the shell, and end-to-end IME.
See [docs/ROADMAP.md](../docs/ROADMAP.md).
