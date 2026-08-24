# SOL Engineering Decision Log

Decisions are recorded as Architecture Decision Records (ADRs) before implementation locks in.

## Spike reports

- **Phase 0 compositor spike** — see [`0010-phase0-compositor-spike.md`](0010-phase0-compositor-spike.md).
  A `sol-compositor` binary + `test-client` prove a full Wayland protocol
  round-trip through Smithay on the winit backend. **Status: accepted
  foundation spike; not a daily-use or release-readiness claim.**
- **Phase 2 SolUI architecture spike** — see
  [`0004-slint-as-solui-substrate.md`](0004-slint-as-solui-substrate.md).
  A retained SolUI controller projects semantic/token state into a private
  Slint component under a headless fixture. **Status: Complete, with explicit
  hardware/accessibility/license follow-ups.**

## ADR list

| # | Decision | Status | Target phase | ADR |
|---|---|---|---|---|
| 1 | Repository strategy: early monorepo | Accepted | Phase 0 | [0001](0001-monorepo.md) |
| 2 | Rust workspace layout | Accepted | Phase 0 | [0002](0002-rust-workspace-layout.md) |
| 3 | SOL Shell not based on Quickshell | Accepted | Phase 0 | [0003-shell-not-quickshell.md](0003-shell-not-quickshell.md) |
| 4 | Slint-backed retained/reactive SolUI | Accepted | Phase 2 | [0004-slint-as-solui-substrate.md](0004-slint-as-solui-substrate.md) |
| 5 | Compositor dev path: winit-first, DRM deferred | Accepted | Phase 0/1 | [0005-compositor-dev-path.md](0005-compositor-dev-path.md) |
| 6 | Compositor ↔ Shell IPC: structural split, transport deferred | Accepted | Phase 0/1 | [0006-shell-ipc-deferred.md](0006-shell-ipc-deferred.md) |
| 7 | IME: first-party sol-ime frontend + fcitx5 engine | Accepted | Phase 1 | [0007-ime-frontend-fcitx5-engine.md](0007-ime-frontend-fcitx5-engine.md) |
| 8 | Historical distribution decision + XWayland dropped | Partially superseded; X11 scope accepted | Phase 1+ | [0008-distribution-xwayland-scope.md](0008-distribution-xwayland-scope.md) |
| 9 | Settings storage and stable minimum API boundary | Accepted | Phase 2 | [0011-settings-storage-api.md](0011-settings-storage-api.md) |
| 10 | Application identity and lifecycle contracts | Accepted | Phase 2 | [0012-application-identity-lifecycle.md](0012-application-identity-lifecycle.md) |
| 11 | Typed System Action and permission layer | Accepted (API contract) | Phase 4 | [0013-system-action-permission-layer.md](0013-system-action-permission-layer.md) |
| 12 | Local, privacy-preserving search index | Accepted (foundation) | Phase 4 | [0014-local-search-index.md](0014-local-search-index.md) |
| 13 | System overlays and layer-shell popup contract | Accepted (headless contract) | Phase 4 | [0015-layer-shell-overlays-popup-contract.md](0015-layer-shell-overlays-popup-contract.md) |
| 14 | Privacy-bounded diagnostics foundation | Accepted (Phase 5 foundation) | Phase 5 | [0016-diagnostics-foundation.md](0016-diagnostics-foundation.md) |
| 15 | SolKit stability policy, SDK tiers, and monorepo review | Accepted (Phase 6 policy) | Phase 6 | [0017](0017-solkit-stability-tiers.md) |
| 16 | Historical pacman/AUR package backend | Superseded by ADR-0020 | Phase 6 | [0018](0018-package-backend.md) |
| 17 | SOL OS product, image, and boot boundary | Accepted (architecture) | Phase 7 | [0019](0019-os-product-and-boot-boundary.md) |
| 18 | SOL package manager, `.app`, and shared runtime | Accepted (architecture) | Phases 8–9 | [0020](0020-sol-package-app-runtime.md) |
| 19 | Default-deny application security and permissions | Accepted (policy) | Phase 8 | [0021](0021-application-security-permissions.md) |
| 20 | System-managed accounts and credential vault | Accepted (architecture) | Phase 8 | [0022](0022-system-managed-accounts.md) |
| 21 | SOL fluid material system | Accepted (design contract) | Phases 2/4/9 | [0023](0023-sol-fluid-material.md) |
| 22 | GTK, Qt, and non-native toolkit compatibility | Accepted (architecture) | Phase 9 | [0024](0024-non-native-toolkit-compatibility.md) |
| 23 | Shell spatial grammar, global menu, and Live Capsule | Accepted (product/architecture) | Phases 4/9 | [0025](0025-shell-spatial-menu-live-capsule.md) |
| 24 | SOL boot execution, UKI, and seamless graphics handoff | Accepted (architecture) | Phase 7 | [0026](0026-sol-boot-uki-and-graphics-handoff.md) |

## Decision register (PRD §41)

Numbers remain stable for cross-document references. Rows marked **Accepted**
are closed at the stated contract level; rows naming a phase without an
accepted decision remain open implementation or product work.

| # | Decision | Earliest prototype trigger | Notes |
|---|---|---|---|
| 3 | Is Smithay renderer sufficient long-term? | Phase 0/1 | Start with Smithay renderer; reassess with damage/VRR/HDR data |
| 4 | Long-term role of Vulkan / wgpu | Phase 1+ | Do not rewrite the renderer prematurely |
| 5 | Compositor ↔ Shell IPC transport / wire format | **Accepted (implementation open)** | [ADR-0006](0006-shell-ipc-deferred.md) selects D-Bus; schema, compositor service, Shell proxy, and integration remain Phase 1 work |
| 6 | Settings storage architecture | **Accepted (Phase 2)** | [ADR-0011](0011-settings-storage-api.md): typed API; daemon-owned storage boundary |
| 7 | Application identity format | **Accepted (Phase 2)** | [ADR-0012](0012-application-identity-lifecycle.md): validated reverse-DNS `AppId`; lifecycle boundary |
| 8 | SolKit ABI/API stability strategy | **Accepted (Phase 6 policy)** | [ADR-0017](0017-solkit-stability-tiers.md): source-API semver begins only with a post-v0.1 public release; no Rust ABI promise |
| 9 | Server-side vs client-side decorations | Phase 1 | Wayland-first policy; affects GTK/Qt/Electron behavior |
| 10 | Global menu protocol | **Existence/placement accepted (Phase 4)** | [ADR-0025](0025-shell-spatial-menu-live-capsule.md): Shell-rendered foreground menu at upper-left; schema remains open |
| 11 | Window tiling product model | Phase 1/4 | Floating + snap first; optional advanced tiling |
| 12 | Application sandbox implementation | **Policy accepted (Phase 8)** | [ADR-0021](0021-application-security-permissions.md): default deny, kernel enforcement, typed portals; exact LSM composition remains open |
| 13 | Upstream intake cadence and SOL release channels | Phase 7 | SOL owns the signed output; exact upstream cadence remains open |
| 14 | Installer and disk layout | Phase 7 | Must preserve A/B and recovery invariants from ADR-0019 |
| 15 | Store/package backend | **Re-decided (OS rebaseline)** | [ADR-0020](0020-sol-package-app-runtime.md): `sol-packaged` + signed `.app`; Software is a client |
| 16 | Search index architecture | **Accepted (Phase 4 foundation)** | [ADR-0014](0014-local-search-index.md): explicit local application catalog; no filesystem/document/clipboard/network indexing; typed launch results only |
| 17 | System Action API | **Accepted (Phase 4 API contract)** | [ADR-0013](0013-system-action-permission-layer.md): typed caller-attributed actions, default-deny grants, consent boundary, and audit; system adapters remain deferred |
| 18 | Crash reporting / diagnostics | **Accepted (Phase 5 foundation)** | [ADR-0016](0016-diagnostics-foundation.md): typed source attribution, deterministic redaction, bounded local stores; collection, upload, consent, and live crash capture remain deferred |
| 19 | IME engine/frontend integration boundary | Phase 1 | fcitx5 addon coverage; sol-ime frontend owns candidate-window UI; engine upgrade strategy; when a custom engine is ever considered |
| 20 | `.app` container encoding and compression | Phase 8 | Identity, signed manifest, contents, and transaction semantics are fixed by ADR-0020 |
| 21 | Runtime ABI/schema tooling | Phase 9 | Major + monotonic contract revision + named features are fixed; generator, registry encoding, and IPC schema tools remain open |
| 22 | Boot measurement, key enrollment, and EFI encoding | **Partially accepted (Phase 7)** | [ADR-0026](0026-sol-boot-uki-and-graphics-handoff.md) fixes the x86-64 UEFI/UKI execution and graphics-handoff architecture; key enrollment, measurement, and revocation remain open |
| 23 | System-image filesystem and delta encoding | Phase 7 | Read-only signed images and atomic slot activation are fixed by ADR-0019 |
| 24 | Account vault storage and hardware sealing | Phase 8 | System ownership and scoped handles are fixed by ADR-0022; backend/TPM API remains open |
| 25 | Fluid-material compositor path | Phase 4/9 | Roles/fallbacks are fixed by ADR-0023; sampling/refraction implementation remains open |
| 26 | Toolkit adapter/protocol matrix | Phase 9 | Support levels and isolation are fixed by ADR-0024; adapter coverage and material schema remain open |
| 27 | Live Activity/menu/status IPC | Phase 4/9 | Placement, attribution, leasing, privacy priority, and trusted rendering fixed by ADR-0025 |

## Related reading

- [Project roadmap →](../ROADMAP.md) (phase-by-phase execution view)
- [PRD ↗](../PRD.md) (product requirements)
