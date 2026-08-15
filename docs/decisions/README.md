# SOL Engineering Decision Log

Decisions are recorded as Architecture Decision Records (ADRs) before implementation locks in.

## Spike reports

- **Phase 0 compositor spike** — see [`0010-phase0-compositor-spike.md`](0010-phase0-compositor-spike.md).
  A `sol-compositor` binary + `test-client` prove a full Wayland protocol
  round-trip through Smithay on the winit backend. **Status: Complete.**

## ADR list

| # | Decision | Status | Target phase | ADR |
|---|---|---|---|---|
| 1 | Repository strategy: early monorepo | Accepted | Phase 0 | [0001](0001-monorepo.md) |
| 2 | Rust workspace layout | Accepted | Phase 0 | [0002](0002-rust-workspace-layout.md) |
| 3 | SOL Shell not based on Quickshell | Accepted | Phase 0 | [0003-shell-not-quickshell.md](0003-shell-not-quickshell.md) |
| 4 | Slint as SolUI substrate (spike pending) | Accepted | Phase 0/2 | [0004-slint-as-solui-substrate.md](0004-slint-as-solui-substrate.md) |
| 5 | Compositor dev path: winit-first, DRM deferred | Accepted | Phase 0/1 | [0005-compositor-dev-path.md](0005-compositor-dev-path.md) |
| 6 | Compositor ↔ Shell IPC: structural split, transport deferred | Accepted | Phase 0/1 | [0006-shell-ipc-deferred.md](0006-shell-ipc-deferred.md) |
| 7 | IME: first-party sol-ime frontend + fcitx5 engine | Accepted | Phase 1 | [0007-ime-frontend-fcitx5-engine.md](0007-ime-frontend-fcitx5-engine.md) |
| 8 | Distribution (pacman/AUR) + XWayland dropped | Accepted | Phase 1+ | [0008-distribution-xwayland-scope.md](0008-distribution-xwayland-scope.md) |

## Open decisions (PRD §41 — to settle during prototyping)

| # | Decision | Earliest prototype trigger | Notes |
|---|---|---|---|
| 1 | SolUI rendering architecture | Phase 2 | Must not leak Smithay specifics into SolUI |
| 2 | Retained vs reactive/declarative UI model | Phase 2 | Affects SolApp, animation, accessibility trees |
| 3 | Is Smithay renderer sufficient long-term? | Phase 0/1 | Start with Smithay renderer; reassess with damage/VRR/HDR data |
| 4 | Long-term role of Vulkan / wgpu | Phase 1+ | Do not rewrite the renderer prematurely |
| 5 | Compositor ↔ Shell IPC transport / wire format | Phase 1 | Structural split resolved in 0006; choose transport (D-Bus / Wayland protocol / shm ring) |
| 6 | Settings storage architecture | Phase 2/3 | Separate Settings UI → Settings API → services |
| 7 | Application identity format | Phase 2/3 | Needed for launcher, commands, notifications, store |
| 8 | SolKit ABI/API stability strategy | Phase 6 | No stability promise in v0.1 |
| 9 | Server-side vs client-side decorations | Phase 1 | Wayland-first policy; affects GTK/Qt/Electron behavior |
| 10 | Global menu | Phase 4 | Optional; do not let it drive initial shell IPC |
| 11 | Window tiling product model | Phase 1/4 | Floating + snap first; optional advanced tiling |
| 12 | Application sandbox default policy | Phase 5/6 | Evaluate SOL sandbox or reuse portals; MVP does not require |
| 13 | SOL Stable vs Arch rolling sync strategy | Phase 5+ | Not an MVP blocker |
| 14 | Installer | Phase 6 | Not an MVP blocker |
| 15 | Store backend | Phase 4+ | Hide package implementation details behind pacman/AUR |
| 16 | Search index architecture | Phase 4 | Command/Action API is the interface |
| 17 | System Action API | Phase 4 | Typed actions + permission layer; shared by search/automation/AI |
| 18 | Crash reporting / diagnostics | Phase 5 | Must not require arbitrary shell access |
| 19 | IME engine/frontend integration boundary | Phase 1 | fcitx5 addon coverage; sol-ime frontend owns candidate-window UI; engine upgrade strategy; when a custom engine is ever considered |

## Related reading

- [Project roadmap →](../../ROADMAP.md) (phase-by-phase execution view)
- [PRD ↗](../../PRD.md) (product requirements)
