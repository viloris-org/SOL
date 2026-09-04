# SOL Roadmap

> **Status:** Living document — this file is refined as each Phase closes.
> **Last reviewed:** 2026-09-02, against the PRD, OS Platform Definition,
> Shell contract, and accepted ADRs through ADR-0025.
> **Basis:** [PRD §38 Development Phases](PRD.md) define the goal and success
> criterion for each Phase.
> **Related:** normative OS contracts in the
> [OS Platform Definition](os-platform.md), Shell behavior in the
> [Shell Spatial and Live Activity Contract](shell-experience.md), engineering
> decisions in the [decision log](decisions/README.md), and product
> requirements in the [PRD](PRD.md). Evidence from the retired frontend is
> preserved in the historical
> [Wayland protocol matrix](status/wayland-protocol-matrix.md). ADR-0028 and
> ADR-0032 supersede those compatibility milestones: current compositor work
> is SCP-only.
>
> This Roadmap is an **engineering execution view** of the PRD: it decomposes
> each Phase into shippable work items, acceptance points, and milestones, and
> flags dependencies and risks. Granularity can (and should) be refined as we
> reach the corresponding Phase.

When documents disagree, the PRD owns product scope, `os-platform.md` owns the
OS trust/package/security/runtime contract, accepted ADRs own engineering
decisions, and this file owns sequencing and closure evidence. Component
READMEs and tests describe implementation evidence; they do not relax a
normative acceptance gate.

> **Migration note (2026-08-28):** Phase 0 and early Phase 1 entries below
> preserve evidence from the retired Wayland spike. They are historical, not
> current acceptance criteria. New compositor, application, shell, clipboard,
> input-method, and toolkit work must use SCP and must pass
> `scripts/validate-scp-only.sh`.

---

## Overview

| Phase | Name | Goal | Success criterion (from PRD §38) | Status |
|---|---|---|---|---|
| 0 | Foundation | Native SCP session | Complete an authenticated SCP surface/toplevel round trip | ✅ Headless protocol foundation accepted |
| 1 | Desktop Core | Visible native SCP desktop | SOL runs a visible, input-capable SCP session on supported hardware | ⏳ Native renderer/input/output closure pending |
| 2 | SolKit | Native app framework | Build an app with native SOL look and interaction entirely in SolKit | ⏳ In progress (real-platform closure pending) |
| 3 | Initial First-party Applications | Core first-party apps | Files, Terminal, and Settings share a unified UX | ⏳ In progress |
| 4 | Shell Experience | Complete desktop interaction model | SOL forms a complete, coherent desktop interaction model | ⏳ In progress |
| 5 | Daily Driver | Long-term daily use | Developers can use SOL as their primary desktop long-term | ⏳ In progress (foundations) |
| 6 | Developer Platform | Ecosystem & SDK stability | Third-party devs build high-quality native apps without knowing SOL internals | ⏳ In progress |
| 7 | OS Foundation | Bootable and recoverable SOL system | Failed manager/deployment trials retain a boot path, and recovery does not depend on the manager it repairs | 🔲 Planned |
| 8 | Native App Platform | Transactional `.app`, explicit permission, and managed account platform | Compatible app resolution and coordinator-atomic grants survive rollback/crash without authority or data rollback | 🔲 Planned |
| 9 | Runtime & Ecosystem | Compact apps and coherent system integration | External apps use major/revision/feature runtime contracts without weakening isolation or accessibility | 🔲 Planned |
| 10 | Professional Applications | Hyperion, Phoebe, and Iapetus | Production-scale AI development and creative workflows meet professional quality, safety, and performance gates | 🔲 Planned |

> **OS rebaseline (2026-08-22):** Phases 0–6 describe the desktop substrate and
> remain useful engineering history. SOL is now a complete Linux-kernel OS.
> Phases 7–9 add the boot, image, package, security, and stable runtime work
> required by that product boundary. See [OS Platform Definition](os-platform.md).

### Status, maturity, and closure rules

Binary checkboxes are no longer used as evidence of product completion. Every
deliverable has a maturity stage, and may advance only when the evidence for
that stage is linked from the deliverable or its status matrix.

| Stage | Name | Required evidence |
|---|---|---|
| S0 | Planned | Scope exists, but no accepted contract or implementation is claimed. |
| S1 | Contracted | Behavior, versions, ownership, failure semantics, and explicit non-goals are accepted. An ADR or model alone stops here. |
| S2 | Implemented | Production code exists and focused automated tests pass. A registered protocol global, renderer-neutral model, fake backend, or scaffold stops here unless integrated. |
| S3 | Integrated | The real components communicate across their intended process/backend boundary and the user-visible lifecycle works end to end in deterministic integration tests. |
| S4 | Validated | The feature is exercised at every boundary promised by the phase: real compositor/rendering, real services, assistive technology, hardware, fault injection, or external clients as applicable. |
| S5 | Release-ready | Required compatibility, performance, accessibility, security, recovery, negative-path, and regression gates pass on the release matrix. |

Phase-level status uses the following rules:

| Mark | Meaning |
|---|---|
| ✅ Complete | Every required closure item is S5 and has reviewable evidence. No required item is deferred to a later phase. |
| ⏳ In progress | At least one implementation slice exists, but one or more required items remain below S5. |
| ⚠ Accepted spike | A historically useful architecture or feasibility result is accepted, without implying product readiness. |
| 🔲 Planned | Architecture or product scope may be accepted, but no phase-level implementation completion is claimed. |

A phase cannot close on a mock, renderer-neutral fixture, headless protocol
round trip, isolated D-Bus test, build-only check, or undocumented manual
claim when its success criterion promises a usable product boundary.

### Definition of Done for every deliverable

Every required deliverable must record:

1. exact scope, non-goals, owner, and dependencies;
2. protocol/API names and supported versions;
3. success, cancellation, restart, disconnect, invalid-input, and cleanup paths;
4. the real user action → process boundary → backend → visible result chain;
5. unit, integration, interoperability, and real-boundary tests as applicable;
6. exact evidence: CI job, test, hardware report, fault-injection report, or
   external-consumer artifact;
7. remaining blockers and the maturity stage they prevent.

An item with missing fields cannot be promoted to S4 or S5.

As part of the 2026-08-24 rebaseline, legacy checked items were conservatively
reset to `[S2]` and legacy unchecked items to `[S0]`. `[S2]` means only that an
implementation or accepted contract was previously recorded; each item still
requires its own evidence audit before promotion. This reset intentionally
removes every inherited completion claim outside the explicitly audited Phase
0/1 tables.

### Current closure gates

| Phase | Blocking closure evidence | Immediate unlock |
|---|---|---|
| 0 | Historical spike accepted; its unclosed product behaviors are owned by Phase 1 | No current release claim |
| 1 | Complete window/layer lifecycle, protocol baseline, compositor↔Shell IPC, native transfer/IME, representative external apps, and real DRM multi-output validation | A defensible basic daily-use compositor baseline |
| 2 | Native renderer/input pacing plus a real AT-SPI/screen-reader session | Treat SolKit as a platform-validated app framework rather than a renderer-neutral contract |
| 3 | Native Files/Terminal/Settings surfaces and Files desktop integrations | Three applications can serve as a real SolKit conformance suite |
| 4 | Native Dock/Launcher/Overview/Notification surfaces, Shell IPC, live thumbnails, global menu/status/Live Capsule, and real gesture input | Complete desktop interaction model |
| 5 | Physical hardware matrix, suspend/resume, display hotplug/scaling, native data transfer, capture, IME, and authorized system writes | Daily-driver claim |
| 6 | Published/versioned SDK, external consumer build, migration/API docs, debugger and native `.app` packaging path | Independent third-party development claim |
| 7 | Reproducible signed image plus deterministic and OVMF fault-injected Stage-0/manager/deployment/recovery trials | Recoverable OS image |
| 8 | Transactional `.app` activation plus kernel/broker enforcement and coordinator-atomic permission/account tests | Native application trust boundary |
| 9 | Stable runtime descriptor/ABI/IPC, external signed app proof, and compatibility conformance | Runtime/ecosystem release |
| 10 | Shared pro-app foundations plus production-scale Hyperion/Phoebe/Iapetus workflow, recovery, performance, color, AI, extension, and interchange validation | Professional first-party application release |

### Delivery tracks and dependency gates

Phase numbers express product maturity, not a requirement to serialize all
work. The following tracks may proceed in parallel, but may merge only at the
named gate.

| Track | Execution order | Merge gate |
|---|---|---|
| Desktop closure | Phase 2 → Phases 3/4 → Phase 5 → Phase 6 external proof | Phase 3/4 native surfaces use the same platform-validated SolKit and Shell contracts |
| OS trust | M7.1 trust/formats → M7.2 deployment state machine → M7.3 Stage-0/recovery → M7.4 release evidence | No artifact is called known-good before authenticated health, data compatibility, and rollback-index gates |
| App trust | M8.1 bundle/store → M8.2 activation → M8.3 security → M8.4 accounts → M8.5 hardening | App ID, publisher, bundle hash, process generation, grants, and leases remain correlated end to end |
| Runtime/ecosystem | M9.1 runtime contract → M9.2 SDK delivery → M9.3 compatibility | M9.1 descriptor schema is required by M8.2 resolution; M8 security/package services are required by the external-app release proof |
| Professional apps | M10.1 shared foundations → M10.2 Hyperion / M10.3 Phoebe / M10.4 Iapetus in parallel → M10.5 release gate | Phase 10 consumes the released runtime, sandbox, GPU/media, account, background-task, and extension contracts from Phases 5–9 |

The OS MVP is the first integrated release slice. It joins the Phase 5 desktop
baseline with M7 recovery, M8 package/security/account enforcement, and one
`sol-runtime-1` external sample from M9. It is not reached merely by completing
the phases in numeric order.

### Normative traceability

| Source contract | Roadmap execution owner |
|---|---|
| [PRD](PRD.md) §33, §38, §41 | Hardware themes, Phase goals/success criteria, and open decision gates across all phases |
| [OS Platform Definition](os-platform.md) §3–4 | M7 boot, recovery, image, and transaction milestones |
| [OS Platform Definition](os-platform.md) §5, §7–8 | M8 bundle, package, sandbox, atomic permission, account, and vault milestones |
| [OS Platform Definition](os-platform.md) §6, §9–11 | M9 runtime, compatibility, and integrated acceptance gates |
| [Shell contract](shell-experience.md) §1–10 | Phase 4 native Shell surfaces and M9.3 stable external menu/status/Live Activity integration |
| [ADR-0019–0025](decisions/README.md) | Accepted invariants and the remaining implementation/non-claim boundaries for M7–M9 |
| [Architecture](architecture.md) | Repository ownership and dependency-direction checks for every implementation slice |

**Do-not-defer-to-the-end themes across phases** (PRD §33): Multi-monitor,
Fractional Scaling, Suspend/Resume, NVIDIA, Touchpad, Display Hotplug, IME.
All must be true by the end of Phase 5; the architecture must not block their
later implementation.

---

## Phase 0 — Native SCP foundation ✅

> **Goal:** establish an authenticated, capability-scoped SCP transport and
> native reference client without a compatibility frontend.

### Accepted evidence

| Deliverable | Maturity | Evidence and limit |
|---|---|---|
| Dev environment / Rust workspace (monorepo, ADR-0001/0002) | S3 | Workspace builds and tests as an integrated repository; release reproducibility is owned by Phase 7. |
| SCP Unix transport | S3 | Authenticates peer PID/AppId, frames bounded messages, and transfers buffer FDs with `SCM_RIGHTS`. |
| `ScpState` | S3 narrow | Owns sessions, capabilities, surface/toplevel/layer objects, and rejects forged tokens and cross-session object access. |
| `scp-client` reference client | S3 fixture | Completes connect → surface → toplevel → configure/ack against the real server. |
| Headless integration tests | S3 fixture boundary | `cargo test -p sol-compositor --test scp_session` validates native round trips without claiming visible rendering. |
| SCP-only regression guard | S3 | `scripts/validate-scp-only.sh` rejects retired packages, socket variables, manifests, and source paths. |
| `sol-design` token seed | S2 | Token consistency tests exist; this is not a compositor closure item. |

### Acceptance baseline

The foundation demonstrates a real SCP client/server boundary, capability
checks, and object lifecycle. It is headless and does not prove visible
rendering, physical input, or a usable hardware session.

### Phase 0 → Phase 1 handoff

The reusable core is `ScpState`. Phase 1 owns connecting SCP-owned surfaces to
native rendering, input, output, shell policy, and a real session.

---

## Phase 1 — Desktop Core ⏳

> **Status:** Rebaselined to SCP-only on 2026-08-28.

> **Goal:** evolve from the headless SCP foundation into a visible,
> input-capable native desktop on supported hardware.

### Audited implementation baseline

This table records the strongest currently evidenced maturity.

| Capability | Current maturity | Evidence and open boundary |
|---|---|---|
| SCP toplevel creation/configure | S3 narrow integration | Native clients complete authenticated headless configure/ack round trips; visible multi-window behavior remains open. |
| Native renderer and composition | S0 | SCP buffers/state exist, but no active renderer presents them to a display. |
| Input and window policy | S0–S2 by model | SCP input state and renderer-neutral shell models exist; physical event ingestion, hit testing, focus, move/resize, and workspaces are not integrated. |
| Native output/DRM topology | S0 | Output protocol state exists; DRM/KMS discovery, modesetting, hotplug, and multi-output composition require a new native backend. |
| Fractional scale | S2 model | SCP output scale can be represented; cross-output rendering and hardware sharpness remain open. |
| SCP layer surface and Shell | S3 narrow | Shell obtains a capability, configures a top layer surface, and commits headlessly; mapping and rendering remain open. |
| Compositor↔Shell typed D-Bus | S1 | ADR-0006 selects D-Bus. The compositor service, schema/bindings, Shell proxy, reconnect, and integration test remain unimplemented. |
| SCP clipboard/drag state | S2 | Capability-scoped state exists; cross-process FD transfer and application adapters remain open. |
| SCP text input | S0 | `sol-ime` model/engine transport exists, but native focus/preedit/commit messages and candidate surface routing remain open. |
| `sol-ime` ↔ fcitx5 | S2 | Model/transport fixtures exist. Candidate rendering and real multilingual desktop validation remain open. |

### M1 success criterion

> "SOL can run a visible, input-capable SCP desktop session on supported hardware."

This criterion is not satisfied by handler registration, a model, or a
headless round trip. M1 closes only when all of the following are S5:

1. **Window lifecycle:** representative SCP-native applications can create
   multiple windows and complete focus, popup,
   move, resize, maximize, restore, fullscreen, minimize, close, disconnect,
   and invalid-request paths with correct visible geometry and SCP state.
2. **Shell lifecycle:** top bar and the Phase-1-required Shell surface are
   actually mapped, laid out, rendered, and focus-safe; the typed D-Bus
   contract drives workspace/window state; Shell crash, restart, and reconnect
   do not disrupt the compositor or applications.
3. **Protocol baseline:** every required SCP capability has a declared version,
   semantic coverage, negative tests, and representative native-client evidence.
4. **Data and input:** clipboard and drag-and-drop work between real
   applications; SCP text input completes an application →
   compositor → fcitx5 → candidate UI → application commit flow for at least
   Chinese, Japanese, and Korean scenarios.
5. **Outputs and scaling:** a two-output session supports placement, cross-
   output movement, hotplug fallback, unplug cleanup, and 1.0/1.25/1.5/2.0
   scale transitions without stale output state or unusable windows.
6. **Real session:** local VT/libseat/DRM/GBM smoke passes on the initial Intel
   and AMD target matrix. NVIDIA status is recorded explicitly and cannot be
   represented as complete without its own evidence.
7. **Regression gate:** all Phase 1 CI tests pass from a clean checkout; the
   hardware/interop report identifies the exact commit, devices, drivers,
   clients, SCP versions, failures, and waivers.

Anything not satisfying these gates remains Phase 1 work; it may not be moved
to Phase 5 solely to preserve a completed Phase 1 label.

---

## Phase 2 — SolKit ⏳

> **Status:** In progress

> **Goal:** form a complete **native application development framework** so
> third-party developers can build apps with SOL-native look and interaction
> (PRD §17–23).

### Milestone M2 deliverables

**SolUI & rendering architecture**

- [S2] **Decision #1/#2 (ADR-0004):** Slint-backed SolUI spike completed;
      architecture settled as retained semantic state projected to a private
      reactive/declarative Slint adapter. Repeatable headless adapter and
      scale/animation fixtures live in `sdk/sol-ui`; real GPU, accessibility,
      multi-output, popup/input-region, and distribution-license validation
      remain explicitly tracked in ADR-0004 rather than being claimed here.
- [S2] Semantic component system: `Button`, `TextField`, `Toolbar`, `Tab`, `TabBar`, `HStack`, `VStack`, plus Liquid Glass surface/button/segmented-control/toolbar/slider/shared-container-morph contracts
       (PRD §18); apps write intent, not visuals
- [S2] Layout engine (`HStack` / `VStack` semantic layout, PRD §18) — Implemented
- [S2] `sol-design` full token convergence: typography, spacing, radius,
      material, motion, shadows, color (PRD §19, §19.1)
- [S2] Liquid Glass renderer contract: semantic material roles, one-level
      backdrop grouping, reduced-transparency/high-contrast fallbacks, and
      secure capability-aware composition plans (ADR-0023). Native compositor
      shaders and hardware/visual validation remain open.
- [S2] Consistency testing: golden-snapshot asserts component-tree output
      contains only token values (tests in sol-design) (§19.1)

**SolAnimation**

- [S2] Unified animation engine (sol-animation): MotionSpec, AnimationDriver, InterruptibleAnimation (PRD §14): easing / spring / interactive
      progress / velocity / interruption / reversal
- [S2] Semantic motion tiers: `Motion::None/Fast/Panel/Material/Morph/Rebound/Window/Workspace`
- [S2] One set of animation semantics shared by compositor and UI: MotionSpec, Motion tiers via sol-animation/sol-design

**SolApp & lifecycle**

- [S2] Application lifecycle (PRD §20): App, AppWindow, AppState
- [S2] Command architecture (PRD §21): `file.open` / `edit.copy` … auto-exposed
      to menus / shortcuts / command palette
- [S2] **Decision #7:** application identity and lifecycle contracts
      (validated reverse-DNS `AppId`; checked process lifecycle; ADR-0012)

- [S2] sol-graphics abstraction (§35): Renderbuffer, Surface, Brush, Paint, GraphicsContext
      (accessibility enters the architecture early)

**Keyboard / accessibility / theme**

- [S2] Standard keyboard interaction & focus management implemented uniformly
      by `sol-ui` (§19.1 behavior consistency): ordered traversal skips
      disabled controls; Enter/Space activate; arrows select tabs; editable
      fields handle text insertion and backspace.
- [S2] Accessibility semantic tree, reduced motion, high contrast (§35):
      renderer-neutral role/state tree and token-mode contract cover focus,
      selection, editability, reduced motion, high contrast, and named text
      scaling in repeatable tests.
- [S2] Theme switching touches only `sol-design` (§19.1 single source of
      truth): components retain token roles while `TokenMode` resolves theme
      and accessibility variants.

> **Platform limit:** the SolUI semantic tree is ready to map into an
> accessibility bridge, but real SCP surface/input integration and
> screen-reader/AT-SPI validation remain integration work; no system assistive-tech
> claim is made by the headless tests.

**Settings boundary**

- [S2] **Decision #6:** settings storage and stable minimum API boundary
      (typed `SettingsApi`; daemon-owned persistent store; ADR-0011)

### M2 success criterion

> "It is possible to fully develop an app with SOL-native look and
> interaction using SolKit." Judged by: an app with layout / components /
> commands / animation / keyboard navigation written entirely with `solkit`
> (`sol-app` + `sol-ui` + `sol-design` + `sol-animation`), without touching a
> concrete renderer.

> **Decision gate (PRD §41):** this Phase settles #1 SolUI rendering
> architecture, #2 retained/declarative, #6 settings storage architecture
> (settled: ADR-0011),
> #7 application identity format (settled: ADR-0012).

### M2 acceptance evidence and closure gate

- [S2] **SolKit workflow example:** `examples/solkit-showcase` creates an app
      using only `sol-app`, `sol-ui`, `sol-design`, `sol-animation`, and the
      renderer-neutral `sol-graphics` contract. Its deterministic CLI/test
      covers layout/components, command execution, interruptible motion,
      token-mode accessibility preferences, keyboard navigation, and the
      semantic accessibility tree without importing a concrete backend.
- [S0] **Native SCP application surface:** connect the private SolUI adapter to
      an SCP toplevel, submit buffers, and drive its event loop against the
      compositor. The current showcase proves only the renderer-neutral API.
- [S0] **Accessibility-platform closure:** verify a real accessibility bridge
      with assistive technology. The native smoke does not prove GPU pacing,
      input latency, multi-output behavior, or AT-SPI/screen-reader transport.
  - [S2] **AT-SPI bridge foundation:** optional AccessKit Unix integration maps
        SolUI roles, labels, values, focus, state, and actions onto a real
        isolated AT-SPI bus; an AT-SPI client traverses and verifies the exported
        application tree. A screen-reader desktop session remains required for
        final closure.

**M2 remains in progress until the real-platform closure item is evidenced.**
The example proves the framework API workflow; it must not be mistaken for a
claim that the unavailable SCP display and assistive-technology environment passed.

---

## Phase 3 — First-party Applications

> **Goal:** ship three first-party apps (Files / Terminal / Settings) and
> dogfood SolKit (PRD §24–27).

### Milestone M3 deliverables

- [S2] **Settings (PRD §26) foundation:** typed Appearance (theme / high
      contrast / reduced motion / text scale) and Sound controls, command
      palette, keyboard/accessibility tree, and settingsd round-trip; layered
      UI → Settings API → system services. Display and input pages explicitly
      report unavailable until their typed service APIs exist.
  - [S2] **Private settings persistence:** `FileSettingsStore` uses atomic
        replacement, private `0600` files, and parent-directory sync after
        rename; reload and permission tests cover the daemon-owned profile.
- [S2] **Terminal (PRD §27) core:** direct-exec PTY/process lifecycle, ANSI/VT
      grid with Unicode and true color, bounded scrollback/search, tabs,
      selection/clipboard and keyboard/resize contracts, renderer-neutral
      SolUI/graphics projection, and command palette navigation. Native
      SCP/GPU rendering, PTY read-loop wiring, and system clipboard smoke
      validation remain platform-adapter follow-ups.
- [S0] **Files (PRD §25):** sidebar / tabs / search / drag & drop / preview /
      removable storage / network locations / context actions / trash /
      keyboard navigation — the **dogfooding baseline (§19.1)**
  - [S2] **Renderer-neutral Files core:** directory tabs, list/grid sorting,
        multi-selection and keyboard navigation, address breadcrumbs, local
        copy/move/rename, recoverable-trash and drag/drop contracts, typed
        errors, command palette, and temp-fixture operation tests.
  - [S2] **Files surface foundation:** SolUI toolbar/tab/sidebar/search/context
        projections, dynamic directory tabs, accessibility semantics, and
        bounded local text/image/binary/metadata preview data are covered by
        deterministic fixtures.
  - [S2] **Bounded image thumbnails:** local PNG/JPEG/GIF/WebP previews decode
        into renderer-neutral RGBA thumbnails capped at 256 px with strict
        dimension/allocation limits; malformed images fall back to binary.
  - [S0] **Desktop and platform integrations:** native SCP/GPU rendering,
        removable and network locations, portal-backed trash, and real desktop
        drag/drop transport.
- [S2] Dogfooding loop (§24): first-party command-palette divergence found in
      Settings / Terminal / Files → shared renderer-neutral SolUI palette
      contract → all three apps now share it, with deterministic dogfood tests.
- [S2] Command palette / keyboard navigation consistent across the three apps:
      `Ctrl+Shift+P`, query filtering, Tab / Shift+Tab traversal, Enter / Space
      activation, Escape dismissal, empty results, and accessibility projection
      are supplied by the same SolUI contract; each app retains typed execution.

**Dogfooding iron rules (§19.1)**

- All first-party apps build via `solui` components + `sol-design` tokens only;
  no hand-rolled visual parameters or interaction components.
- `sol-files`, as the most complex app, carries the polish baseline; new
  components mature there first, then sink back into `sol-ui`.

### M3 success criterion

> "The three first-party apps share a unified UX instead of each implementing
> system behavior themselves." Judged by: the three apps share the same
> tokens / components / commands / keyboard behavior, with no bare
> hex/ms/f32 visual parameters mixed in.

> **Prerequisites:** Phase 2 SolKit must be complete enough to support
> Terminal's GPU rendering and Files' complex lists / drag-drop. Settings
> depends on backend services (settingsd) — developed in parallel with
> Phase 4/5 service capabilities.

---

## Phase 4 — Shell Experience

> **Goal:** deliver a complete, coherent desktop interaction model
> (PRD §11, §28, §29).

### Milestone M4 deliverables

- [S2] **Dock / Launcher foundation:** renderer-neutral pinned/running app model,
      deterministic app catalog, typed launch / activate / close requests, and
      SolUI keyboard/accessibility navigation. Real compositor activation and
      close adapters remain unimplemented and explicitly report unavailable.
  - [S0] **Native SOL Dock surface:** bottom-centered solid panel, Launcher
        entry, pinned/running/focused state, badges, drag ordering, optional
        auto-hide, active-output behavior, and compositor activation/minimize.
  - [S0] **Application Launcher surface:** authenticated `.app` grid/library,
        Dock-anchored interruptible presentation, `Super+A`, keyboard/a11y,
        fractional scaling, and reduced-motion behavior.
  - [S0] **Left-side window controls:** native and server-side decorations use
        Close / Minimize / Maximize-Restore; GTK/Qt adapter conformance must
        preserve compositor-owned chrome without a generic CSD fallback.
- [S0] **Overview / Workspace:** workspace overview, visual switching
  - [S2] **Renderer-neutral overview core:** typed workspace/window snapshots,
        accessibility and keyboard model, switch/move-window intents, and a
        compositor-bridge contract with repeatable fixtures.
  - [S2] **Native overview surface contract:** validated output/fractional-scale
        boundary, bounded thumbnail projection, deterministic card layout,
        accessibility tree, raster frame, lifecycle, and typed host dispatch
        now live in `shell::overview_surface`. Real compositor IPC, live window
        thumbnails, and layer-shell presentation remain open.
  - [S0] **Native overview surface:** compositor IPC adapter, real window
        thumbnails/layout, and presentation on a layer-shell surface.
- [S2] **Top Bar foundation:** renderer-neutral clock/date, workspace, network,
      audio, power, and privacy/activity provider contracts; unavailable/stale/
      error state, keyboard/accessibility navigation, and permission-gated typed
      intents. Read-only NetworkManager/PipeWire/UPower status adapters are
      validated; compositor activation and write-capable system adapters remain
      deferred.
- [S0] **Top-bar spatial contract (ADR-0025):** foreground app identity/global
      menu fixed at upper-left; Live Capsule, typed status items, Notification
      Center, and system status fixed at upper-right across narrow, localized,
      scaled, and multi-display layouts.
- [S0] **Global application menu:** compositor-focus-authenticated App ID,
      atomic command snapshots, overflow, keyboard/accessibility, SolKit command
      export, and GTK/Qt public menu/action adapters.
- [S0] **Typed application status/tray registry:** authenticated declarative
      icons/state/actions, Shell-owned rendering/overflow/rate limits, and a
      constrained legacy bridge with no embedded arbitrary client windows.
- [S0] **Live Capsule service and surface:** one upper-right anchor multiplexing
      leased declarative live activities; typed Open/Pause/Resume/Stop/End
      actions; privacy-first ordering; crash/expiry cleanup; keyboard/a11y;
      solid surface and anchored interruptible expansion.
  - [S0] **Broker-authoritative privacy capsules:** microphone, camera, screen
        capture, location, and remote-control state comes from real capability
        leases, cannot be hidden/replaced by apps, and Stop/Revoke terminates
        the underlying broker session.
  - [S0] **Application registration:** declared `shell.live-activity` plus
        explicit atomic permission; registration grants presentation only and
        cannot acquire media/capture/background authority.
- [S2] **Notification service foundation:** typed `NotificationApi` +
      `sol-notificationd` lifecycle, replacement, action, query, and storage
      boundary, including a Shell-consumed `NotificationDbusProxy` adapter
      validated against the real daemon on an isolated session bus.
- [S0] **Notification Center** (wired to `sol-notificationd`)
  - [S2] **Renderer-neutral center core:** typed `NotificationApi` adapter,
        application/urgency grouping, lifecycle dismissal/actions, keyboard,
        accessibility semantics, and repeatable service-adapter fixtures. An
        isolated `dbus-run-session` test proves `NotificationCenter` drives
        the real `sol-notificationd` through `NotificationDbusProxy` for
        grouping, action invocation, user dismissal, and retained history.
  - [S2] **Notification service adapters:** `org.sol.Notifications1` exposes
        caller-attributed typed notification publish, replacement, query,
        action-validation, and dismissal flows. The daemon also implements
        standard `org.freedesktop.Notifications` methods and emitted
        `NotificationClosed` / `ActionInvoked` signals through the same
        owner-checked records. Standard `app_name` / `desktop-entry` metadata
        is validated as claimed app identity, not authentication; isolated
        session-bus checks cover both protocols and signals.
  - [S0] **Native notification surface:** layer-shell presentation, user policy,
        and real application action callback delivery.
- [S2] **Quick Settings** (wired to `sol-settingsd`)
  - [S2] **Renderer-neutral quick settings core:** typed volume/mute with
        `SystemActionApi` authorization, appearance/accessibility preferences,
        keyboard/accessibility semantics, and fixture-backed adapters.
  - [S2] **Settings service adapter:** the `org.sol.Settings1` session-bus
        service exposes only complete typed snapshots and named setting changes;
        `SettingsDbusProxy` implements `SettingsApi`, with an isolated
        `dbus-run-session` service/client round trip.
  - [S2] **Quick Settings daemon integration:** the real Shell model uses
        `SettingsDbusProxy` against an isolated `sol-settingsd`, applying
        appearance directly and volume/mute only after typed authorization;
        the daemon snapshot proves all three mutations persisted.
  - [S2] **Read-only system status adapters:** typed network, Bluetooth, and
        audio-device status are available without granting mutation authority.
    - [S2] **Read-only PipeWire output inventory:** structured `pactl` JSON maps
          validated output IDs, descriptions, running/idle/suspended state,
          default membership, active ports, and port availability into a typed
          Shell contract. Deterministic rejection fixtures and a live host
          query cover the installed USB output without changing audio state.
          Device switching and all writes remain open.
    - [S2] **Read-only NetworkManager status:** the Shell `NetworkProvider`
          reads the system-bus global state, active connection identity, and
          Wi-Fi/wired link quality through typed D-Bus properties. Unknown or
          inconsistent states become explicit provider errors; network writes,
          and device switching remain open.
    - [S2] **Read-only BlueZ status:** the Shell `BluetoothProvider` reads local
          adapters and remote devices from the system-bus ObjectManager,
          validates identities, state, and optional battery percentages, and
          exposes a deterministic renderer-neutral snapshot. Pairing,
          connecting, disconnecting, discovery control, and all other BlueZ
          writes remain open.
- [S0] **Touchpad gestures (§13 / §4.4):** four-finger workspace switching etc.,
      gesture progress → UI progress
  - [S2] **Renderer-neutral workspace gesture model:** the overview controller
        handles interruptible progress, velocity-aware settling, cancellation,
        adjacent-workspace bounds, and reduced-motion behavior with fixtures.
        libinput gesture events, compositor dispatch, and hardware latency
        validation remain required.
- [S2] **Search & Launcher (§28) foundation:** private local application catalog,
      deterministic explainable ranking, and permission-gated typed launch
      execution. File/document/clipboard/command/calculator providers and the
      `Super+Space` desktop shortcut remain explicit follow-up adapters.
  - [S2] **Native four-surface desktop loop:** the real `sol-shell` process maps
        an SCP background, top bar, bottom-centered Dock, and on-demand
        Launcher; routes compositor pointer/keyboard events; follows output
        changes; recreates closed persistent surfaces; and refreshes live
        system status without cadence-driven full-desktop repainting. The
        authenticated installed-app catalog, production action/window adapter,
        compositor-owned global shortcuts, reconnect validation, and physical
        GPU/display presentation remain open.
  - [S2] **Gesture progress core:** interruptible/cancellable progress, velocity
        handoff, semantic workspace settling, and reduced-motion fixtures.
  - [S0] **Real input integration:** libinput gesture adapter, compositor IPC
        dispatch, and hardware/touchpad latency validation.
- [S2] **Command / Action API (§21):** typed action + permission layer shared by
      search / automation / accessibility. **API contract accepted in
      ADR-0013:** caller-attributed action catalog, default-deny grants, trusted
      consent boundary, and audit are covered by deterministic fixtures;
      concrete portal/polkit/system-service adapters remain deferred.
- [S2] **System overlay / popup contract:** renderer-neutral OSD, menu,
      popover, and modal/scrim roles now have typed output, anchor,
      exclusive-zone, input-region, focus, Escape/dismiss, accessibility, and
      token-motion contracts with deterministic SolUI fixtures (ADR-0015).
      Screen-recording and IME candidate-window product surfaces remain open.
- [S2] **SCP layer-surface popup model validation (ADR-0004 validation point #1):**
      repeatable headless compositor + SolUI fixture validates placement,
      fractional scale, input, focus, and lifecycle contracts. The existing
      `sol-shell --once` exercises the real SCP desktop background, Dock, and
      top-bar configure/commit path (the Launcher maps on demand);
      automated cross-process popup, physical multi-output, GPU, and AT-SPI
      validation remain field work rather than CI claims.

### M4 success criterion

> "SOL forms a complete, coherent desktop interaction model." Judged by: dock /
> launcher / global menu / right-side status and Live Capsule / overview /
> notifications / quick settings / touchpad gestures working under one
> animation + token system, with interruptible and reversible interactions and
> broker-authoritative privacy indicators.

> **Decision gate (PRD §41):** #10 global menu existence/placement is settled
> by ADR-0025; its IPC schema remains implementation work. #11 tiling remains
> open. #16 search is settled by ADR-0014; #17 System Action is settled at the
> API-contract level by ADR-0013. Production adapters and desktop-session
> validation remain follow-on work.

---

## Phase 5 — Daily Driver

> **Goal:** let developers use SOL long-term as their primary desktop. **All
> "do-not-defer" themes settle here.**

### Milestone M5 deliverables

**Stability & hardware (hard settlement, §33)**

- [S0] **Suspend / Resume:** session restores correctly; surfaces/state intact
  - [S2] **Checkpoint and restoration core:** `sol-session` validates a
        generation-tagged surface/workspace checkpoint, persists it through a
        typed store, and enforces suspend/resume ordering with deterministic
        tests. logind PrepareForSleep, DRM/libseat revoke, process quiescing,
        and real desktop restore remain required.
- [S0] **Multi-monitor:** hotplug, independent configuration, per-monitor
      workspaces
- [S0] **Fractional scaling:** crisp rendering at non-integer scales
  - [S2] **Fractional-scale state boundary:** SCP output configuration validates
        scale values and SolUI converts logical sizes at the host boundary.
        Per-output surface updates, physical GPU rendering, and display
        sharpness validation remain required.
- [S0] **NVIDIA:** driver path, private GBM / VRAM parameters
- [S0] **Touchpad / gestures:** mature gesture stack
- [S0] **Display hotplug:** complete
- [S0] **Audio / Bluetooth / Power** (PipeWire / BlueZ / UPower integration)
  - [S2] **UPower status adapter:** the Shell top-bar provider reads UPower's
        aggregate display device over the real system bus, distinguishes a
        battery-less desktop from a zero-percent battery, rejects malformed or
        unknown device state, and has a live host-service validation script.
  - [S2] **PipeWire audio status adapter:** the Shell top-bar provider consumes
        structured `pactl` JSON from PipeWire's Pulse compatibility service,
        validates the declared default sink and channel volumes, and exposes a
        read-only typed output/port inventory checked against the live user
        service. Device switching and authorized writes remain open.
  - [S2] **BlueZ status adapter:** the Shell reads adapter power/discovery state
        and paired/connected remote-device state through the system-bus object
        manager, with strict validation and an optional live-service smoke
        test. Pairing, connection changes, discovery control, and other BlueZ
        writes remain open.

**Desktop core capabilities**

- [S2] **SCP session-launch foundation:** installed `sol-session` validates an
      XDG runtime directory and deterministic `SOL_SCP_SOCKET`. Standalone mode
      can start a compositor for development; production login attaches user
      services and `sol-shell` to the boot compositor after `sol-logind`
      authorizes the PAM-authenticated UID. The lock remains engaged until the
      Shell commits a complete desktop and the transparent handoff finishes.
      Native
      rendering, libseat/DRM, VT, and field desktop-session validation remain
      required.
- [S0] Clipboard, drag & drop fully polished
  - [S2] **Renderer-neutral clipboard and drag foundation:** Terminal exposes a
        typed `ClipboardAdapter` with a deterministic memory fixture, and
        Files validates typed `DropRequest` copy/move operations against local
        fixtures. Native SCP data-device transport and desktop smoke
        validation remain required.
  - [S2] **Native SCP clipboard state foundation:** the SCP state machine owns
        capability-checked selection and drag/drop messages. Cross-process FD
        transfer, application adapters, persistence, and native desktop-session
        behavior remain open.
- [S2] **Portal authorization foundation:** `sol-portal` maps typed document
      open and screen-capture requests through the caller-attributed
      `SystemActionApi`; default-deny and explicit authorization are fixture
      tested without granting arbitrary portal work.
  - [S2] **Portal authorization D-Bus adapter:** `org.sol.Portal1` accepts only
        validated caller identity and document-open/screen-capture intents,
        returning decisions and correlation IDs without exporting executable
        authorization tokens. An isolated daemon/proxy test proves default
        deny and malformed-request rejection.
- [S0] **Screen sharing / screen recording:** XDG portal D-Bus, file chooser
      UI, protected-content-aware compositor/PipeWire adapters, stream
      lifecycle, and desktop-session validation remain required. Display
      scanout and physical audio sink monitors are forbidden capture sources.
  - [S2] **Authorized ScreenCast lifecycle core:** `sol-portal` consumes only a
      matching private `PortalAuthorization`, enforces create → select
      sources → start → close ordering, accepts only compositor-produced safe
      feeds, validates their one-to-one transport stream mapping, and owns both
      stages' cleanup. XDG portal interfaces, picker UI, real streams, and
      desktop validation remain open.
  - [S2] **Protected capture composition foundation:** SCP separates local
      display and exportable capture passes. Broker-marked DRM/privacy/auth
      surfaces become opaque compositor-owned placeholders before their buffer
      is read, preserving unrelated regions without revealing obscured windows.
      Authenticated broker IPC, native protected GPU buffers, KMS/HDCP loss
      handling, capture effects, and real PipeWire publication remain open.
  - [S2] **Capture-safe audio policy foundation:** `sol-audiod` plans a separate
      recording mix from allowed per-playback nodes and excludes protected
      nodes fail-closed. The native PipeWire graph adapter and live-session
      validation remain open; a physical sink monitor may not substitute.
- [S0] SOL toolkit adapter matrix (GTK / Qt / SDL / Flutter / Electron)
  - [S0] Each supported toolkit needs a bundled SOL adapter that targets SCP
        explicitly. The retired compatibility probes are removed; no toolkit
        receives an implicit legacy display socket.
- [S0] **IME complete (§21.1):** stable end-to-end flow for mainstream languages
  - [S2] **First-party frontend and fcitx5 transport foundation:** `sol-ime`
        owns typed preedit/candidate state, keyboard selection, and a live
        `org.fcitx.Fcitx.InputContext1` adapter with deterministic engine
        fixtures. Real SCP text-input capability wiring,
        fcitx5 availability, and mainstream-language desktop validation remain
        required.

**Security & diagnostics**

- [S2] **Privacy-bounded diagnostics foundation:** `sol-diagnostics` records
      typed source/severity/code events with deterministic summary redaction,
      bounded retention, and private local storage; ADR-0016 prohibits shell
      access and opaque payloads at this boundary.
- [S0] **Live crash reporting:** authenticated service transport, real crash
      capture, consent UX, encrypted export/upload policy, and field validation
      remain required before treating diagnostics as a production reporter.
  - [S2] **Shell panic-capture foundation:** the real Shell startup installs a
        process-local panic hook that persists a typed fatal/process-crash event
        through the private bounded diagnostics store. A child-process test
        proves an actual Rust panic is redacted and written before exit. Signal
        capture, authenticated transport, consent, encryption, upload, and
        desktop-session validation remain open.
- [S2] **Permission grant persistence foundation:** `FilePermissionStore`
      persists caller/capability allow or deny grants through the typed action
      boundary with atomic private files and repeatable revocation tests.
- [S2] **Authorization audit persistence foundation:** `FileActionAuditStore`
      durably preserves typed authorization decisions in private atomically
      replaced files with strict round-trip validation.
- [S0] **Production permission model (§31 / ADR-0021):** trusted consent UI,
      kernel/broker policy, minimum-scope permission atoms, and one durable
      grant + audit + lease/consumption transaction remain. The current
      separate stores are intentionally not considered production-atomic.
  - [S2] **Trusted consent surface foundation:** a renderer-neutral Shell prompt
        presents the exact caller, source, capability, typed action, and policy
        rationale; SolUI keyboard/accessibility choices resolve allow-once,
        allow-always, or deny only through `SystemActionApi`, with persistence
        and audit fixtures. Native trusted presentation, cross-process policy,
        and the remaining protected capabilities remain open.

### M5 success criterion

> "Developers can use SOL as their primary desktop environment long-term."
> Judged by: sustained stable use across daily scenarios (multi-monitor,
> external desktop or laptop, suspend/resume, NVIDIA or AMD), with IME and
> share/record available.

> **Historical gate:** the former package-repository installation target is
> retained as a transitional build check only. ADR-0019 through ADR-0022 move
> production boot, packages, and sandbox enforcement into Phases 7–9.

---

## Phase 6 — Developer Platform

> **Goal:** build a third-party developer ecosystem and SDK stability promise
> (PRD §17, §23, §30, §42).

### Milestone M6 deliverables

- [S0] **Public SolKit:** public API polish and versioning
  - [S2] **Pre-release public boundary validation:**
        `scripts/validate-solkit-public-api.sh` checks version alignment,
        unpublished package metadata, library targets, and dependency direction
        for the five Public-tier crates. Registry publication and external
        consumer compatibility remain release-gate work.
- [S2] **SDK stability policy (§41 #8):** ADR-0017 defines post-v0.1 Public
      source-API semver gates, rejects a Rust ABI promise, and makes no claim
      that the current unpublished crates are stable.
- [S0] **Documentation:** getting started, guides, API reference
      (`sol-sdk-docs`)
  - [S2] **Current SDK API map:** `docs/solkit-getting-started.md` and
        `docs/solkit-api.md` document the copy-out workflow, public/restricted
        crate boundaries, and the locked rustdoc generation command. Published
        versioned API reference and migration guides remain open.
- [S2] **Starter template:** `templates/solkit-starter` provides a public-crate
      app skeleton, copy-out dependency instructions, and deterministic
      external-copy validation (`scripts/validate-solkit-starter.sh`)
- [S2] **Project scaffolding:** `scripts/new-solkit-project.sh` creates a named
      external starter, validates its package and app identity, and is covered
      by `scripts/test-new-solkit-project.sh`
- [S2] **Templates:** `templates/solkit-component` adds a library-only,
      Public-tier SolUI/sol-design component template with external-copy and
      scaffolding validation; publication, stability, native rendering, and
      packaging remain separate work.
- [S0] **Developer tools:** scaffolding, debugging, packaging tools
  - [S2] **SDK environment doctor:** `scripts/solkit-doctor.sh` validates the
        toolchain, a locked Cargo manifest, starter copy-out behavior, and an
        optional full workspace check without modifying the target project.
- [S2] **Historical Store backend decision:** ADR-0018 recorded the former
      pacman/AUR direction; ADR-0020 supersedes it for the OS rebaseline.
- [S2] **SDK permission tiers (§23):** ADR-0017 formalizes Public, Restricted,
      and Private contracts and their dependency direction.
- [S2] **Monorepo review (§39):** ADR-0017 retains the monorepo until a public
      SDK release, independent consumers, enforceable boundaries, and an
      independently versioned component justify a split.

### M6 success criterion

> "Third-party developers can build high-quality native apps without
> understanding SOL internals." Judged by: an external developer uses SolKit
> templates + docs to independently build an app with the currently available
> SDK workflow. Native `.app` delivery is the Phase 9 production gate.

> **Non-goals:** self-built kernel / init / audio / network stacks, commercial
> store/payments, full AI assistant, mobile support, office suite, or premature
> multi-major runtime support.

---

## Phase 7 — OS Foundation

> **Goal:** boot, update, validate, roll back, and recover a SOL-owned system
> image on conforming x86-64 UEFI without a curated certification matrix.

### M7.1 — Trust topology, formats, and reproducibility

- [S0] Select the minimal Stage-0 implementation and EFI entry layout. Platform
      recovery must be firmware-addressable without a working `sol-boot`, and a
      signed external recovery path must cover ESP/storage loss.
- [S0] Close PRD §41 decisions #13, #22, and #23 for release channels,
      key enrollment, measured boot, replay-resistant rollback storage, and
      system-image encoding without weakening ADR-0019 invariants.
- [S0] Finish distinct canonical schemas for manager/recovery trials,
      deployment placement, Android-like priority/bootable/tries/successful
      state, authenticated health checkpoints, revocation, and rollback epochs.
- [S2] Define allocation-free format-1 deployment state and boot-success
      encodings with strict canonical parsing, monotonic redundant-copy
      sequencing, CRC32 torn-write detection, and byte-stable migration
      fixtures. The implemented CRC format is torn-write detection only and is
      not the production state authenticator or anti-replay mechanism.
- [S2] Extend the installed deployment schema for a content identity independent
      of A/B placement, complete UKI digest, logical kernel/initrd identities,
      dm-verity root, key epoch, security version, and compatibility constraints
      without reinterpreting manifest format 1.
- [S2] `sol-image` manifest foundation: reproducible deployment manifests bind
      the current development slot and generation to kernel, initrd, root-image
      SHA-256 digest/length, and sorted runtime major/revision/feature
      descriptors.
      Canonical parsing, atomic output, and mutation fixtures reject drift in
      every bound artifact; signing, final image/UEFI encoding, and composition
      remain separate gates.
- [S0] Produce an inspectable build manifest/SBOM and a reproducibility report
      from two isolated builds; document any allowed non-deterministic fields.

### M7.2 — Deployment transaction, health, and rollback

- [S0] Implement the `resolve → fetch → verify → stage → validate → commit`
      transaction with the inactive slot written first and its manifest
      committed last.
- [S2] `sol-boot-core` remains the firmware-independent deployment selector:
      consume each trial before transfer and bind observations to the exact
      deployment/generation/attempt. It is not evidence that manager self-
      recovery, report authentication, or anti-rollback is complete.
- [S0] `sol-boot` verifies artifacts, selects only a complete signed deployment,
      enforces bounded retry, and falls back to a retained known-good slot.
- [S0] Production uses one complete UKI containing kernel, initrd, immutable
      command line, and release metadata; an external-initrd development ISO is
      not production verified-boot evidence.
- [S0] `sol-boot` may draw one static centered mark in the unchanged current GOP
      mode. It never reads EDID, chooses a mode, or calls `SetMode()`; graphics
      failure is ignored and Linux DRM owns native resolution and interaction.
- [S0] Early userspace reports an authenticated measured identity and reaches
      distinct verified-root, repairability, shared-data compatibility, and
      promotion checkpoints for the exact unpredictable attempt.
- [S0] Advance the replay-resistant security rollback index only after
      promotion. A failed unpromoted trial must still boot the retained version.
- [S0] Irreversible shared-data or firmware migrations happen after the rollback
      barrier or use a snapshot/versioning contract the retained image can read.
- [S0] Power loss, partial download/write, signature failure, corrupt manifest,
      failed health gate, and stale/replayed boot-success reports leave the
      previous deployment selected and user data unchanged.

### M7.3 — Stage-0, manager, and independent recovery

- [S0] Ship a stable signed Stage-0 that can select retained/trial signed manager
      copies and automatically reach platform recovery when neither runs.
- [S0] Keep platform recovery independently firmware-addressable. It can repair
      Stage-0, the ESP, managers, and deployment state without executing the
      manager being repaired; signed external recovery covers device loss.
- [S0] Support automatic exhaustion, durable software requests, and a firmware
      or physical manual recovery action. A request is acknowledged only after
      recovery starts, so power loss cannot silently consume it.
- [S0] Implement separate two-phase manager and recovery updates: write inactive,
      verify, register one-shot trial, then promote or retain the old copy.
- [S0] Deployment-paired recovery boots without the compositor or Shell and can
      repair its compatible deployment/data without implicitly lowering another
      deployment's trust policy.
- [S0] Garbage collection retains a manager, recovery, and deployment fallback
      until its replacement has passed the corresponding trial gate.

### M7.4 — Installation and release evidence

- [S0] Generic x86-64 UEFI installer with explicit disk layout,
      encryption, Secure Boot/key enrollment, recovery-key, reinstall, and data
      preservation behavior.
  - [S2] **Live-session welcome surface:** `sol-installer` provides a native,
        token-resolved entry page with explicit Install / Keep exploring exits,
        a truthful no-disk-changes message, an accessible semantic tree, and a
        concise preview of disk, encryption/Secure Boot, and final-review
        decisions. Disk discovery and the installation transaction remain
        outside this bounded UI deliverable.
- [S0] Deterministic fault injection and OVMF cover clean install; interrupted
      manager/recovery/deployment update; corrupt image; failed trial; firmware-
      variable failure; power loss at every commit boundary; stale/replayed
      state; automatic fallback; manual recovery; and data preservation.
- [S0] Boot graphics tests prove only that missing/broken GOP cannot affect boot
      policy and that optional static drawing never uses EDID or `SetMode()`.
      SOL maintains no certified boot-graphics hardware matrix and makes no
      native-resolution, flicker-free, or seamless-handoff release claim.
- [S0] Publish a signed release-evidence manifest recording exact artifacts,
      deterministic/OVMF coverage, known failures, waivers, and retained paths.

### M7 dependencies and non-claims

- **Inputs:** ADR-0019 and `os-platform.md` §3–4; trust-root/key-enrollment
  policy, disk layout, image encoding, Stage-0, and recovery entry topology are
  required before M7.4 can close. A manually curated certified hardware matrix
  is explicitly not an input.
- **Parallelism:** image composition and state-machine fault injection can run
  without the graphical desktop; recovery UX must not depend on Phase 4.
- **Non-claim:** a QEMU boot or signature check alone does not prove firmware
  fallback, power-loss safety, known-good promotion, or data preservation.

### M7 success criterion

> A failed staged manager, recovery, or deployment update cannot remove every
> authorized path: Stage-0 can reach a retained manager or platform recovery,
> recovery does not depend on the manager it repairs, security rollback remains
> blocked, and shared data remains usable by the retained deployment.

**Required closure evidence:** one clean install, one successful update, and the
full failure matrix above pass in deterministic and OVMF fault-injection
coverage. Real-machine results are useful smoke evidence, not a certified boot-
graphics compatibility class.

---

## Phase 8 — Native Application Platform

> **Goal:** make `.app` the signed, isolated, transactional unit of native SOL
> application installation and execution.

### M8.1 — Bundle, repository, and content store

- [S0] Close PRD §41 decision #20 and define a canonical `.app` manifest plus
      deterministic container encoding. Signature-covered fields include App ID,
      publisher, executable/resource hashes, architecture, capabilities,
      extensions, runtime major, minimum contract revision, and features.
- [S0] `sol-bundle` build/lint/inspect/sign/verify tools emit SBOM/provenance and
      reject non-canonical input, undeclared executable content, install hooks,
      path traversal, ambiguous identity, and unsupported runtime requirements.
- [S0] `sol-pkg` client and privileged `sol-packaged` service consume signed
      repository metadata with publisher trust, revocation, rollout/channel
      policy, transparency data, offline verification, and a content-addressed
      machine-wide read-only store.
- [S0] Correlate repository identity → bundle hash → installed record → launched
      process; filenames, mutable URLs, desktop metadata, and app-supplied names
      are never authentication.

### M8.2 — Transactional lifecycle and compatibility resolution

- [S0] Atomic install/update/remove/rollback preserves app data outside the
      bundle and leaves the previous active version intact on interruption.
- [S0] Maintain separate preferred and effective state. Resolve the first
      non-revoked compatible hash from the recorded fallback chain against the
      booted deployment's authenticated runtime descriptor, never by display
      version ordering.
- [S0] Test that update prepends, explicit app rollback truncates newer resolution
      candidates, OS rollback never rewrites the preferred pointer, and fresh
      reinstall creates a fresh chain and security identity relationship.
- [S0] Expose an explicit per-app unavailable state when no compatible retained
      hash exists; do not block boot, mutate app data, or silently select an
      incompatible version.
- [S0] Garbage collection retains a compatible app version for every known-good
      deployment when one was previously installed and records the compatibility
      matrix during system-update validation.

### M8.3 — Process isolation, portals, and atomic authority

- [S0] `sol-securityd` authenticates durable and release identities, creates
      isolated data roots, and enforces default deny with namespaces, cgroups,
      seccomp, SCP capability enforcement, and the selected Landlock/LSM composition.
- [S0] Replace the Phase 5 grant/audit prototype stores with one authoritative
      ledger where the minimum-scope grant, audit record, and lease or allow-once
      consumption commit together or not at all.
- [S0] Same-lineage update/rollback may retain eligible durable grants but always
      refreshes release/process-bound handles. New capabilities and publisher
      discontinuity inherit nothing; uninstall/reinstall requires new consent.
- [S0] File, device, media, secret, and other protected capabilities use typed
      brokers/portals and trusted point-of-use Shell consent. Direct service,
      socket, filesystem, and SCP authorization-bypass attempts fail closed.
- [S0] Revocation invalidates authority before cleanup and remains effective
      across app/security-service crashes, stale handles, replay, and offline use.

### M8.4 — Managed accounts and credential vault

- [S0] `sol-securityd` coordinates transaction IDs, prepare/commit/abort/recovery,
      participant receipts, commit proofs, and monotonic authorization generations.
- [S0] `sol-accountsd` owns device/connected account metadata, provider adapters,
      lifecycle, and prepared app × account × scope associations that are not
      enumerable before commit.
- [S0] `sol-vaultd` owns encrypted credentials, hardware-backed sealing where
      available, explicit recovery keys, commit-proof-bound scoped leases, and
      generation-fenced removal. Apps never receive durable credentials.
- [S0] Crash injection before/after every participant and coordinator boundary
      converges idempotently: no partial grant, audit, association, or usable
      credential survives an abort or reported revocation.

### M8.5 — Cross-boundary security release gate

- [S0] Threat model and conformance suite cover undeclared, implicit, bundled,
      partially committed, stale-generation, cross-App-ID, revoked, and direct-
      service authority, plus publisher discontinuity and reinstall semantics.
- [S0] Two bundles with conflicting private dependencies run side by side without
      host-library resolution, cross-app data access, or shared mutable package
      state.
- [S0] Publish transaction/fault-injection evidence for package, permission,
      account, vault, portal, and service-restart boundaries.

### M8 dependencies and non-claims

- **Inputs:** ADR-0012/0013 and ADR-0020 through ADR-0022; M7 supplies the
  authenticated deployment/runtime descriptor, while M9.1 freezes its schema.
- **Trusted UI:** Phase 4's consent and privacy surfaces may be developed in
  parallel, but M8 closes only when their decisions drive real broker authority.
- **Non-claim:** persisted grants, a sandbox command line, or a consent mock by
  itself does not prove atomic authorization or kernel/broker enforcement.

### M8 success criterion

> Two `.app` bundles with conflicting private dependencies run side by side; an
> interrupted update leaves the old version active; OS rollback resolves an
> older compatible app or an explicit unavailable state; undeclared, implicit,
> partially committed, stale-generation, or revoked authority fails at the
> broker boundary; and an app cannot enumerate an account or retain its durable
> credential without an explicit coordinator-committed account-scoped grant.

**Required closure evidence:** end-to-end tests must start from a signed
repository, launch the authenticated installed hash, exercise real enforcement,
and repeat the update/rollback/revocation paths with crash injection.

---

## Phase 9 — Runtime and Ecosystem

> **Goal:** let external developers ship compact native apps by sharing only a
> stable SOL platform while keeping every other dependency private.

### M9.1 — Stable runtime contract

- [S0] Close PRD §41 decision #21 and publish a canonical signed
      `sol-runtime-1` descriptor with stable C-compatible ABI where in-process
      calls are required, versioned IPC, monotonic contract revision, named
      features, architecture, and lifecycle/support policy. Internal Rust ABI is
      explicitly excluded.
- [S0] Generate or validate ABI/API/IPC schemas and compatibility fixtures;
      compatible revisions add without removing old contracts, while breaking
      changes install as side-by-side runtime majors.
- [S0] Cover UI, lifecycle, accessibility, localization, settings, storage,
      notifications, documents, commands, background work, accounts, and typed
      capability-broker clients through stable runtime endpoints.
- [S0] Freeze the descriptor/resolution schema shared with M7 deployment
      manifests and M8 app activation before either compatibility gate closes.

### M9.2 — External SDK and release workflow

- [S0] Finish Phase 6 publication gates: versioned SolKit bindings, API reference,
      migration guide, compatibility tests, and supported-language policy.
- [S0] Ship `.app` project templates, reproducible release pipeline, signing and
      verification workflow, permission/runtime linting, local sandbox runner,
      repository publishing, and debugging/inspection tools.
- [S0] An external developer, using only published docs and artifacts, builds and
      signs the sample outside the monorepo, installs it through `sol-pkg`, and
      exercises accessibility plus at least one document and one brokered
      protected-capability flow.
- [S0] Software catalog remains an unprivileged client of `sol-packaged`; CLI and
      GUI installation share the same trust, transaction, and policy path.

### M9.3 — Toolkit adapters and Shell integration

- [S0] Close PRD §41 decisions #26 and #27 for the adapter/protocol matrix and
      Live Activity/menu/status IPC schema.
- [S0] Publish SOL-adapter recipes for selected GTK, Qt, SDL, Flutter, and
      Electron bundles that vendor their tested non-SOL runtimes and plugins.
- [S0] `sol-gtk` and `sol-qt` adapters map public toolkit APIs to lifecycle,
      documents, notifications, atomic permissions, accounts, appearance,
      accessibility, windowing/decorations, global menus, status items, Live
      Capsule registration.
- [S0] Shell integrations are authenticated, declarative, leased/rate-limited,
      removed on crash/replacement/expiry, and never grant their underlying
      media, capture, device, or background authority.
- [S0] Native / Integrated / Adapted conformance proves identical sandbox,
      denial, account, update, rollback, and fresh-handle semantics; adapters
      load no mutable host toolkit/plugin and fail explicitly when the SCP/SOL
      Runtime adapter is unavailable.

### M9.4 — Runtime and adapter conformance release gate

- [S0] Side-by-side runtime-major and system-rollback tests prove first-compatible
      non-revoked hash selection, explicit unavailable state, protected
      retention, and unchanged app data/preferred pointer.
- [S0] Two GTK/Qt apps with incompatible private toolkit versions coexist with a
      Native sample and receive equivalent system-capability/security behavior.
- [S0] Publish signed runtime descriptors, SDK/tool versions, conformance results,
      compatibility matrix, and known limitations as one release-evidence set.

### M9 dependencies and non-claims

- **Inputs:** Phase 2/6 supply the public framework/API discipline; M8 supplies
  installation, identity, enforcement, permissions, accounts, and portals;
  Phase 4 supplies the trusted Shell surfaces consumed by stable integrations.
- **Non-claim:** an in-tree sample, an unpublished Rust crate, visual theme
  similarity, or one successful toolkit launch does not establish a stable
  runtime/ecosystem contract.

### M9 success criterion

> An external developer builds, signs, installs, runs, updates, and rolls back
> a `.app` that carries only app-specific/non-SOL dependencies and accesses
> protected resources/accounts solely through explicitly granted SOL framework
> capabilities. GTK/Qt apps with
> incompatible private runtimes coexist and retain the same security/system-
> capability guarantees as SolKit apps. Rolling the OS back deterministically
> selects the first non-revoked retained runtime-compatible hash from the
> preferred release's fallback chain or exposes an
> explicit per-app unavailable state without blocking boot or changing app data.

**Required closure evidence:** repeat the external sample lifecycle against the
current and retained known-good deployments, then run Native/Integrated/
Adapted security and accessibility conformance.

---

## Phase 10 — Professional First-party Applications

> **Goal:** ship a professional application tier: Hyperion, an ultra-flagship
> AI IDE; Phoebe, a Lightroom-class photo workflow; and Iapetus, a
> Photoshop-class image editor (PRD §24 and §27A–27C).

### M10.1 — Shared professional application foundations

- [S0] Publish versioned public contracts for large documents, recoverable
      history/autosave, background jobs, GPU compute/tiles, color management,
      model execution/provenance, extension isolation, and project asset links.
- [S0] Establish shared observability and resource budgets for memory, GPU,
      storage, model downloads, cancellation, crash recovery, and thermal or
      battery pressure; degradation must preserve document correctness.
- [S0] Keep application-specific workflow and document models separate while
      moving genuinely shared platform behavior into SolKit/SOL Runtime.

### M10.2 — Hyperion: ultra-flagship professional AI IDE

- [S0] Deliver large-repository multi-language editing, LSP, debugger, tests,
      profiler, source control, terminals, tasks, and local/remote workspaces.
- [S0] Make repository-aware AI planning, editing, building, testing, and
      debugging first-class, with reviewable plans/diffs/tool calls, explicit
      authority, provenance, checkpoints, and isolated parallel agents.
- [S0] Support local/cloud model providers and sandboxed extensions without
      giving either ambient filesystem, process, network, account, or secret
      access.

### M10.3 — Phoebe: professional photo workflow

- [S0] Deliver color-managed RAW ingestion, catalog/search, culling, metadata,
      collections, non-destructive adjustments, masks, presets, history,
      virtual copies, batch editing, and deterministic export recipes.
- [S0] Preserve immutable originals and portable edit metadata; GPU previews,
      background export, AI denoise/selection/search, backup, and recovery must
      remain cancellable, attributable, and restart-safe.

### M10.4 — Iapetus: professional image editing

- [S0] Deliver layers, masks, blend modes, adjustment layers, transforms,
      selections, painting, paths, typography, retouching, filters, automation,
      and recoverable history on tiled large canvases.
- [S0] Validate high-bit-depth, wide-gamut, HDR, and print workflows; sandboxed
      brushes/filters/scripts and AI selection/generation/fill/restoration are
      versioned, attributable, editable operations.
- [S0] Import/export common raster, vector, camera, and layered formats with a
      published compatibility matrix and explicit unsupported-feature handling.

### M10.5 — Professional release gate

- [S0] Run production-scale reference projects through complete edit, save,
      crash/restart, export/build/test, collaboration/interchange, extension,
      update/rollback, and permission-revocation paths.
- [S0] Publish supported hardware/model/file-format matrices and measured
      latency, memory, GPU, color-accuracy, accessibility, recovery, and output-
      fidelity results; no benchmark-class positioning closes on feature count
      or screenshots alone.

### M10 success criterion

> A professional can complete and recover a production-scale software project
> in Hyperion or imaging project in Phoebe/Iapetus. AI actions are attributable
> and reviewable, promised creative edits are non-destructive, extensions are
> sandboxed, and large-project performance, color, accessibility, document
> interchange, and output fidelity pass the published release matrix.

**Dependencies and non-claims:** Phase 10 is post-MVP and depends on the
released Phases 5–9 platform contracts. The three products may proceed in
parallel after M10.1 stabilizes. Naming a feature, embedding a model, opening a
sample image, or rendering a large file once does not establish professional
readiness.

---

## Cross-cutting: technical debt & governance

| Topic | Starts | Notes |
|---|---|---|
| `sol-design` token convergence | Phase 2 | Every component must pass Design Review before entering `sol-ui` (§19.1 rule #2) |
| Consistency CI (golden snapshot) | Phase 2 | Turn "consistency" into a continuously verifiable mechanism (§19.1) |
| App identity format | Phase 2/3 | Prerequisite for launcher/commands/notifications/store (§41 #7) |
| Permission layer (typed action) | Phase 4 | Shared by search/automation/accessibility/AI (§21/§29) |
| Security model | Phase 4–8 | Typed action foundation evolves into ADR-0021 kernel/broker enforcement |
| Atomic permissions | Phase 8 | Each grant is one user/app/capability/resource/duration; grant + audit + lease is one commit |
| Managed accounts | Phase 8 | `sol-accountsd`/`sol-vaultd`; apps receive scoped handles, not durable credentials |
| Shell spatial grammar | Phase 4 | ADR-0025 fixes Dock/menu/window-control/right-zone placement and Live Capsule trust |
| Toolkit adapters | Phase 9 | Bundled private runtime + explicit SOL adapter; capability equality, not pixel-identical widgets |
| Boot / deployment trust | Phase 7 | Stage-0, manager trials, independent recovery, deployment placement, authenticated health, anti-rollback, and data barriers have separate state joined by explicit commits |
| Package identity | Phase 8 | `.app` App ID/publisher/hash must remain correlated from repository to process |
| Runtime compatibility | Phase 9 | Stable major slots; C-compatible ABI + versioned IPC, never internal Rust ABI |
| Professional documents and AI | Phase 10 | Recoverable large-document history, color/GPU correctness, model provenance, sandboxed extensions, and explicit authority are shared release gates |
| Hardware test matrix | throughout | AMD → Intel → NVIDIA; laptop/desktop; single/multi-display; HiDPI (§33) |
| Fault injection | Phases 7–10 | Exercise every persistent transaction boundary, service restart, stale generation, and rollback path before release |
| Release evidence | Phases 6–10 | Signed artifact inventory, exact test matrix, hardware/runtime identifiers, known failures, and explicit waivers |
| Accessibility / localization | throughout | Real AT, keyboard, text scale, contrast, reduced motion/transparency, RTL internals, narrow/scaled/multi-output layouts |

## Long-term platform direction (after Phase 10, PRD §42)

```text
Linux
  ↓
SOL Boot + System Image
  ↓
SOL Security + Package Services
  ↓
SOL Framework Runtime
  ↓
SolKit
  ↓
SOL Applications → Third-party Applications
```

- **Technical-asset focus:** Boot + System Image + Package Manager + Security +
  Compositor + Framework Runtime + Design System + Applications.
- **Direction call:** users should not perceive underlying complexity because
  of Linux or compositor internals — what they see is *SOL*.
- **Ongoing evaluation:** long-term ARM64 support (§32), after the x86-64 boot
  and recovery contract is proven.

---

## Revision history

- **2026-09-02** — Rebased Phase 7 around a stable Stage-0, independently
  reachable recovery, Android-like deployment slot state, promotion-gated
  anti-rollback, shared-data rollback barriers, indivisible production UKIs,
  and best-effort current-GOP presentation. Removed native-resolution,
  seamless-boot, and certified boot-graphics matrix requirements.

- **2026-09-02** — Added the post-MVP professional first-party application
  tier: Hyperion (ultra-flagship professional AI IDE), Phoebe (Lightroom-class
  photo workflow), and Iapetus (Photoshop-class image editor), with shared
  foundations and measurable Phase 10 release gates.

- **2026-08-24** — Reopened Phase 1 after auditing implementation against its
  daily-use success criterion. Replaced binary completion semantics with S0–S5
  maturity stages and a uniform Definition of Done; added the then-normative,
  now-retired Wayland protocol matrix and real-session, interoperability, Shell IPC, transfer,
  IME, multi-output, and hardware closure gates. Earlier Phase 1 “complete”
  entries below are retained as historical records and explicitly superseded.

- **2026-08-23** — Clarified document precedence and closure semantics, added
  current blockers and parallel delivery tracks, and decomposed Phases 7–9 into
  executable sub-milestones with dependencies, non-claims, fault-injection
  coverage, and release-evidence gates aligned to the PRD, OS Platform
  Definition, Shell contract, and ADR-0019 through ADR-0025.

- **2026-08-22** — Rebased SOL from a desktop platform into a
  complete Linux-kernel OS. Added Phases 7–9 for redundant trial-updated
  Stage-0/`sol-boot`/independent recovery, content-identified A/B deployments,
  `sol-pkg`/`sol-packaged`, self-contained `.app` bundles, default-deny sandbox
  permissions, and side-by-side SOL Runtime majors.

- **2026-08-22** — Tightened the OS contract to minimum, explicit, atomic
  permission grants and added system-managed accounts/credential vaults.

- **2026-08-22** — Defined runtime major/revision/feature compatibility and
  per-deployment app fallback, same-publisher grant continuity with fresh
  handles, uninstall/reinstall re-consent, and `sol-securityd`-coordinated
  account/vault participant transactions with generation-fenced revocation.

- **2026-08-22** — Defined Native, Integrated, and Compatible application
  levels. GTK/Qt and other toolkits bundle private runtimes and optional
  toolkit-matching SOL adapters while retaining identical security, account,
  update, and rollback guarantees.

- **2026-08-15 (historical claim; superseded 2026-08-24)** — Phase 1 M1 shell
  and IME slices were recorded as implemented and CI-validated. The evidence
  actually established protocol globals, models, and a headless
  configure/commit round trip; it did not establish rendered layer surfaces,
  compositor↔Shell D-Bus, or end-to-end IME.

- **2026-08-16 (historical claim; superseded 2026-08-24)** — Phase 1 was marked
  complete from implementation slices, a transport decision, build checks, and
  headless fixtures. The reopened Phase 1 section and protocol matrix now own
  its actual maturity and closure gates.
- **2026-08-16** — `sol-ime` gained an fcitx5 session-bus transport and typed
  engine/frontend event boundary. A fake pinyin round-trip covers preedit,
  candidates, and commit without a daemon; the optional D-Bus smoke verifies
  input-context setup and frontend UI signals on a running fcitx5 session.
- **2026-08-16** — Shell gained a read-only NetworkManager status adapter with
  deterministic state validation and an optional system-bus smoke test. The
  Quick Settings network write path remains intentionally unavailable until a
  permission-gated typed action/service API exists.
- **2026-08-16** — The compositor now advertises `wp_fractional_scale_v1`,
  validates per-output fractional scales, and renders at the configured value.
  A headless 1.25x client round-trip verifies the protocol preference.
- **2026-08-16** — Shell gained a read-only BlueZ ObjectManager adapter with
  deterministic adapter/device validation and an optional system-bus smoke
  test. Pairing, connection, and discovery writes remain intentionally open.
