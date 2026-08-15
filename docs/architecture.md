# SOL Architecture

SOL is a layered platform (PRD §5). This page maps that logical layering onto
the current monorepo so new readers can find where a responsibility lives and
understand the boundary rules.

## Logical layers → code

```text
Applications  (apps/)   sol-files · sol-terminal · sol-settings … (Phase 3)
─────────────
SolKit        (sdk/)    sol-ui · sol-app · sol-graphics ·
                        sol-animation · sol-system · sol-design (Phase 2)
─────────────
Runtime       (services/) sol-settingsd · sol-notificationd ·
                          sol-portal · sol-ime
─────────────
Shell         (shell/)  sol-shell — top bar / dock / launcher / overview
                        │  typed IPC (transport TBD, ADR-0006)
─────────────
Compositor    (compositor/) sol-compositor — Smithay / Wayland / scene /
                            WM / input / animation / renderer / shell-ipc
─────────────
Linux         Arch / systemd / Mesa / PipeWire / NetworkManager / BlueZ …
```

## Boundary rules

| Boundary | Rule | Basis |
|---|---|---|
| Compositor ↔ backend | `SolState` owns all protocol state; backends (winit now, udev later) only drive it | ADR-0005, ADR-0006 |
| Compositor ↔ Shell | two processes over typed IPC; a shell crash never kills the compositor | PRD §11, ADR-0006 |
| App → SolKit → renderer | Apps and Shell never touch a renderer/Slint directly; `sol-ui` owns semantic components, `sol-design` owns every visual parameter | PRD §19.1 |
| App → services | Through the `sol-system` (Restricted) API, not direct D-Bus in every app | PRD §23 |
| Monorepo crates | Each crate could eventually split out; no hidden coupling across boundaries | ADR-0001 |

## Key state & flow (compositor)

For the compositor specifically:

- `compositor/src/state.rs` — `SolState`: the Smithay protocol state
  (`wl_compositor`, `wl_shm`, `xdg_shell`, seat, data-device) plus handlers.
- `compositor/src/main.rs` — the `run_winit` backend event loop: render,
  dispatch/flush clients, publish frame callbacks, accept sockets.
- `compositor/examples/test-client.rs` — reference client proving a full
  round-trip.
- `compositor/tests/sol_session.rs` — the end-to-end session test.

Every feature that grows the compositor belongs in `SolState` (as a new
handler/state), not in `main.rs`.

## Rendering & UI stack (target)

```text
sol-ui  (semantic components)   ← apps & shell write intent here
   ↓
Slint (candidate substrate, ADR-0004, spike pending)
   ↓
sol-graphics abstraction → Smithay GlesRenderer (compositor-owned for now)
```

## See also

- [PRD §5 System architecture overview](PRD.md#5-system-architecture-overview),
  [PRD §10 Compositor architecture](PRD.md#10-compositor-architecture)
- [Decision log](decisions/README.md)
- [Roadmap](ROADMAP.md)
