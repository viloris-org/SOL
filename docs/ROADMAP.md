# SOL Roadmap

> **Status:** Living document — this file is refined as each Phase closes.
> **Basis:** [PRD §38 Development Phases](PRD.md) define the goal and success
> criterion for each Phase.
> **Related:** engineering decisions in [`docs/decisions/`](decisions/README.md);
> product requirements in the [PRD](PRD.md).
>
> This Roadmap is an **engineering execution view** of the PRD: it decomposes
> each Phase into shippable work items, acceptance points, and milestones, and
> flags dependencies and risks. Granularity can (and should) be refined as we
> reach the corresponding Phase.

---

## Overview

| Phase | Name | Goal | Success criterion (from PRD §38) | Status |
|---|---|---|---|---|
| 0 | Foundation | Standalone Wayland session | Start a standalone SOL Wayland session and run standard Wayland apps | ✅ **Complete (2026-08-15)** |
| 1 | Desktop Core | A usable daily Wayland compositor | SOL works as a basic daily-use Wayland compositor | ✅ **Complete (2026-08-16)** |
| 2 | SolKit | Native app framework | Build an app with native SOL look and interaction entirely in SolKit | ⏳ In progress (real-platform closure pending) |
| 3 | First-party Applications | First-party apps | The three first-party apps share a unified UX | ⏳ In progress |
| 4 | Shell Experience | Complete desktop interaction model | SOL forms a complete, coherent desktop interaction model | ⏳ In progress |
| 5 | Daily Driver | Long-term daily use | Developers can use SOL as their primary desktop long-term | ⏳ In progress (foundations) |
| 6 | Developer Platform | Ecosystem & SDK stability | Third-party devs build high-quality native apps without knowing SOL internals | ⏳ In progress |

**Do-not-defer-to-the-end themes across phases** (PRD §33): Multi-monitor,
Fractional Scaling, Suspend/Resume, NVIDIA, Touchpad, Display Hotplug, IME.
All must be true by the end of Phase 5; the architecture must not block their
later implementation.

---

## Phase 0 — Foundation ✅

> **Goal:** set up the Arch development environment, Rust workspace, Smithay
> compositor, basic Wayland client, input, and basic rendering.

### Completed

| Deliverable | Status | Evidence |
|---|---|---|
| Arch dev environment / Rust workspace (monorepo, ADR-0001/0002) | ✅ | `cargo check --workspace` passes cleanly |
| `sol-compositor` Smithay compositor (winit backend) | ✅ | Starts a standalone `wayland-sol` session |
| Base Wayland protocols (`wl_compositor` / `wl_shm` / `xdg_shell` / seat / data-device) | ✅ | `SolState` implements all handlers |
| GL rendering + frame-callback loop | ✅ | `.render` loop in `main.rs` |
| `test-client` reference client | ✅ | `examples/test-client.rs` |
| End-to-end integration test | ✅ | `cargo test -p sol-compositor --test sol_session` PASSES |
| winit-first dev path (DRM deferred, ADR-0005) | ✅ | Developable with no root / no VT |
| `sol-design` design-token seed (color/type/spacing/radius/motion/material) | ✅ | `sdk/sol-design/` ships consistency tests |

### Acceptance baseline

> "Start a standalone SOL Wayland session and run standard Wayland
> applications." Demonstrated via `--spawn weston-terminal` and the
> `sol_session` integration test.

### Phase 0 → Phase 1 handoff ✅

Phase 1 is now complete. Phase 0's reusable core (`SolState`) has been extended
with window management, workspace, layer-shell, and IME handling. The DRM/udev
connector and hotplug path now builds and has fixture-backed contracts;
real-hardware DRM/GBM smoke validation remains a hardware follow-up.

---

## Phase 1 — Desktop Core ✅

> **Status:** Complete (2026-08-16)

> **Goal:** evolve from "minimal well-formed compositor" into a **basic
> daily-use Wayland compositor**: window management, focus, move/resize,
> workspaces, multi-monitor, basic shell IPC, and input-method protocols.

### Milestone M1 deliverables

**Window management & scene (WM & Scene)**

- [x] Real hit-testing and pointer focus (replaces the Phase 0 "motion focuses
      the first toplevel" placeholder in `main.rs`)
- [x] Window model: window state / position / size / geometry based on
      `ToplevelSurface` (`compositor/src/window.rs`, `WindowManager`)
- [x] Move / resize (`xdg_toplevel` move/resize interactions) — interactive
      pointer grabs in `compositor/src/grabs.rs`
- [x] Tiling / snapping foundation: Floating + Snap (PRD §12); Tiling optional
      (§41 #11 undecided)
- [x] Keyboard focus switching (Alt+Tab to start); correct
      `Activated`/`Unactivated` delivery

**Workspace & Output**

- [x] Workspace model (PRD §13): surface groups + switching; reserves a
      `WorkspaceTransition` progress interface for touchpad interactive
      transitions (§4.4)
- [x] Output management / basic multi-monitor (enumerate and map the primary
      `wl_output`; work area derived from it; `OutputManagerState` keeps the
      globals alive for future per-monitor modes — do not defer to the end, §33)
- [x] Clear-screen / HiDPI basics (`scale` pass-through from `wl_output`;
      fractional scaling verified in Phase 5)
- [x] Basic display-hotplug handling (§33): `add_output` / `remove_output`
      register/retire a `wl_output` global on connect/disconnect; the
      architecture supports per-monitor modes without a rewrite

**Shell integration & IPC**

- [x] `layer-shell` protocol integration (prerequisite for the shell top bar /
      dock; a key ADR-0004 validation point)
- [x] **Typed IPC decision:** D-Bus chosen among ADR-0006's three options
      (D-Bus / custom Wayland protocol / shm ring) — ADR-0006 accepted
- [x] First shell surface: `sol-shell` renders as a layer-shell top bar
      (validated by the `sol_session` shell round-trip integration test)
- [x] Structural guarantee: a shell crash must not kill the compositor
      (PRD §11 hard constraint) — `sol-shell` is a separate process from
      `sol-compositor` joined only by layer-shell + D-Bus (ADR-0006), so the
      compositor is insulated by construction; the headless integration test
      exercises the two-process boundary

**Input method (IME) — first class (PRD §21.1 / §40)**

- [x] Compositor-side `text-input v3` + `input-method v2` protocol integration
      (note: Smithay 0.7 ships text-input v3 + input-method v2 — the newer v4 /
      v3 staging protocols are evaluated when Smithay raises them; ADR-0007)
- [x] `sol-ime` frontend scaffold: candidate window / preedit data model +
      layout with `sol-design` tokens (visual rendering via `sol-ui` lands in
      Phase 2, when `sol-ui` exists)
- [x] fcitx5 engine transport: `org.fcitx.Fcitx.InputMethod1` session-bus
      input context forwards key events and translates preedit / candidate /
      commit signals into the SOL frontend model. A deterministic pinyin fake
      covers `shan → 山`; an ignored session-bus smoke test is available when
      fcitx5 is running.

### M1 success criterion

> "SOL can be used as a basic daily-use Wayland compositor."
> Judged by: windows can be created/moved/resized/focused, multiple
> workspaces switchable, multi-monitor works, the shell top bar coexists with
> the compositor over the settled IPC, and the IME protocol/frontend seam is
> present. The fcitx5 transport is covered by a deterministic pinyin flow and
> optional session-bus smoke; full compositor-to-surface delivery follows the
> candidate-window UI work.

### M1 dependencies & risks

| Dependency | Notes |
|---|---|
| `layer-shell` protocol | Introduced from `wayland-protocols-wlr` (no clash with `wayland-protocols`); the shell top bar round-trips in the `sol_session` integration test ✅ |
| DRM/udev backend | `--features udev` builds on current Arch; udev/sysfs connector enumeration + hotplug reconciliation are fixture-tested. Real VT + DRM/GBM smoke remains required (ADR-0005). |
| IPC transport | Settled: D-Bus via ADR-0006; shell and compositor are separate processes |

### M1 milestone status

- **Done:** window management core (hit-test/focus/Alt+Tab), move/resize,
  Floating + Snap, workspace model (+ touchpad `WorkspaceTransition` seam),
  layer-shell protocol + shell top bar, D-Bus IPC decision, structural
  shell/compositor split, output management + HiDPI + display-hotplug,
  compositor text-input v3 + input-method v2, sol-ime frontend scaffold +
  fcitx5 D-Bus transport / pinyin fake harness.
- **Not yet (post-M1 / Phase 1 follow-on):** real-hardware DRM/GBM
  multi-monitor smoke across device, connector, and driver combinations
  (ADR-0005). IME candidate-window rendering and full compositor-surface
  delivery remain later UI integration work.

---

## Phase 2 — SolKit ⏳

> **Status:** In progress

> **Goal:** form a complete **native application development framework** so
> third-party developers can build apps with SOL-native look and interaction
> (PRD §17–23).

### Milestone M2 deliverables

**SolUI & rendering architecture**

- [x] **Decision #1/#2 (ADR-0004):** Slint-backed SolUI spike completed;
      architecture settled as retained semantic state projected to a private
      reactive/declarative Slint adapter. Repeatable headless adapter and
      scale/animation fixtures live in `sdk/sol-ui`; real GPU, accessibility,
      multi-output, popup/input-region, and distribution-license validation
      remain explicitly tracked in ADR-0004 rather than being claimed here.
- [x] Semantic component system: `Button`, `TextField`, `Toolbar`, `Tab`, `TabBar`, `HStack`, `VStack`
       (PRD §18); apps write intent, not visuals
- [x] Layout engine (`HStack` / `VStack` semantic layout, PRD §18) — Implemented
- [x] `sol-design` full token convergence: typography, spacing, radius,
      material, motion, shadows, color (PRD §19, §19.1)
- [x] Consistency testing: golden-snapshot asserts component-tree output
      contains only token values (tests in sol-design) (§19.1)

**SolAnimation**

- [x] Unified animation engine (sol-animation): MotionSpec, AnimationDriver, InterruptibleAnimation (PRD §14): easing / spring / interactive
      progress / velocity / interruption / reversal
- [x] Semantic motion tiers: `Motion::None/Fast/Panel/Window/Workspace`
- [x] One set of animation semantics shared by compositor and UI: MotionSpec, Motion tiers via sol-animation/sol-design

**SolApp & lifecycle**

- [x] Application lifecycle (PRD §20): App, AppWindow, AppState
- [x] Command architecture (PRD §21): `file.open` / `edit.copy` … auto-exposed
      to menus / shortcuts / command palette
- [x] **Decision #7:** application identity and lifecycle contracts
      (validated reverse-DNS `AppId`; checked process lifecycle; ADR-0012)

- [x] sol-graphics abstraction (§35): Renderbuffer, Surface, Brush, Paint, GraphicsContext
      (accessibility enters the architecture early)

**Keyboard / accessibility / theme**

- [x] Standard keyboard interaction & focus management implemented uniformly
      by `sol-ui` (§19.1 behavior consistency): ordered traversal skips
      disabled controls; Enter/Space activate; arrows select tabs; editable
      fields handle text insertion and backspace.
- [x] Accessibility semantic tree, reduced motion, high contrast (§35):
      renderer-neutral role/state tree and token-mode contract cover focus,
      selection, editability, reduced motion, high contrast, and named text
      scaling in repeatable tests.
- [x] Theme switching touches only `sol-design` (§19.1 single source of
      truth): components retain token roles while `TokenMode` resolves theme
      and accessibility variants.

> **Platform limit:** the SolUI semantic tree is ready to map into an
> accessibility bridge, but real Wayland screen-reader/AT-SPI transport and
> input-method interaction remain integration work; no system assistive-tech
> claim is made by the headless tests.

**Settings boundary**

- [x] **Decision #6:** settings storage and stable minimum API boundary
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

- [x] **SolKit workflow example:** `examples/solkit-showcase` creates an app
      using only `sol-app`, `sol-ui`, `sol-design`, `sol-animation`, and the
      renderer-neutral `sol-graphics` contract. Its deterministic CLI/test
      covers layout/components, command execution, interruptible motion,
      token-mode accessibility preferences, keyboard navigation, and the
      semantic accessibility tree without importing a concrete backend.
- [x] **Native Wayland session smoke:** the Slint-backed showcase ran against
      a live `sol-compositor` Wayland socket on 2026-08-16. The bounded run
      reached the native event loop without a protocol or renderer failure.
- [ ] **Accessibility-platform closure:** verify a real accessibility bridge
      with assistive technology. The native smoke does not prove GPU pacing,
      input latency, multi-output behavior, or AT-SPI/screen-reader transport.

**M2 remains in progress until the real-platform closure item is evidenced.**
The example proves the framework API workflow; it must not be mistaken for a
claim that the unavailable Wayland and assistive-technology environment passed.

---

## Phase 3 — First-party Applications

> **Goal:** ship three first-party apps (Files / Terminal / Settings) and
> dogfood SolKit (PRD §24–27).

### Milestone M3 deliverables

- [x] **Settings (PRD §26) foundation:** typed Appearance (theme / high
      contrast / reduced motion / text scale) and Sound controls, command
      palette, keyboard/accessibility tree, and settingsd round-trip; layered
      UI → Settings API → system services. Display and input pages explicitly
      report unavailable until their typed service APIs exist.
- [x] **Terminal (PRD §27) core:** direct-exec PTY/process lifecycle, ANSI/VT
      grid with Unicode and true color, bounded scrollback/search, tabs,
      selection/clipboard and keyboard/resize contracts, renderer-neutral
      SolUI/graphics projection, and command palette navigation. Native
      Wayland/GPU rendering, PTY read-loop wiring, and system clipboard smoke
      validation remain platform-adapter follow-ups.
- [ ] **Files (PRD §25):** sidebar / tabs / search / drag & drop / preview /
      removable storage / network locations / context actions / trash /
      keyboard navigation — the **dogfooding baseline (§19.1)**
  - [x] **Renderer-neutral Files core:** directory tabs, list/grid sorting,
        multi-selection and keyboard navigation, address breadcrumbs, local
        copy/move/rename, recoverable-trash and drag/drop contracts, typed
        errors, command palette, and temp-fixture operation tests.
  - [x] **Files surface foundation:** SolUI toolbar/tab/sidebar/search/context
        projections, dynamic directory tabs, accessibility semantics, and
        bounded local text/image/binary/metadata preview data are covered by
        deterministic fixtures.
  - [ ] **Desktop and platform integrations:** native Wayland/GPU rendering,
        removable and network locations, portal-backed trash, image thumbnails,
        and real desktop drag/drop transport.
- [x] Dogfooding loop (§24): first-party command-palette divergence found in
      Settings / Terminal / Files → shared renderer-neutral SolUI palette
      contract → all three apps now share it, with deterministic dogfood tests.
- [x] Command palette / keyboard navigation consistent across the three apps:
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

- [x] **Dock / Launcher foundation:** renderer-neutral pinned/running app model,
      deterministic app catalog, typed launch / activate / close requests, and
      SolUI keyboard/accessibility navigation. Real compositor activation and
      close adapters remain unimplemented and explicitly report unavailable.
- [ ] **Overview / Workspace:** workspace overview, visual switching
  - [x] **Renderer-neutral overview core:** typed workspace/window snapshots,
        accessibility and keyboard model, switch/move-window intents, and a
        compositor-bridge contract with repeatable fixtures.
  - [ ] **Native overview surface:** compositor IPC adapter, real window
        thumbnails/layout, and presentation on a layer-shell surface.
- [x] **Top Bar foundation:** renderer-neutral clock/date, workspace, network,
      audio, power, and privacy/activity provider contracts; unavailable/stale/
      error state, keyboard/accessibility navigation, and permission-gated typed
      intents. NetworkManager/PipeWire/UPower/compositor/portal adapters and
      real desktop activation remain deferred.
- [x] **Notification service foundation:** typed `NotificationApi` +
      `sol-notificationd` lifecycle, replacement, action, query, and storage
      boundary (Shell/D-Bus adapters remain pending)
- [ ] **Notification Center** (wired to `sol-notificationd`)
  - [x] **Renderer-neutral center core:** typed `NotificationApi` adapter,
        application/urgency grouping, lifecycle dismissal/actions, keyboard,
        accessibility semantics, and repeatable service-adapter fixtures.
  - [x] **Notification service adapters:** `org.sol.Notifications1` exposes
        caller-attributed typed notification publish, replacement, query,
        action-validation, and dismissal flows. The daemon also implements
        standard `org.freedesktop.Notifications` methods and emitted
        `NotificationClosed` / `ActionInvoked` signals through the same
        owner-checked records. Standard `app_name` / `desktop-entry` metadata
        is validated as claimed app identity, not authentication; isolated
        session-bus checks cover both protocols and signals.
  - [ ] **Native notification surface:** layer-shell presentation, user policy,
        and real application action callback delivery.
- [ ] **Quick Settings** (wired to `sol-settingsd`)
  - [x] **Renderer-neutral quick settings core:** typed volume/mute with
        `SystemActionApi` authorization, appearance/accessibility preferences,
        keyboard/accessibility semantics, and fixture-backed adapters.
  - [x] **Settings service adapter:** the `org.sol.Settings1` session-bus
        service exposes only complete typed snapshots and named setting changes;
        `SettingsDbusProxy` implements `SettingsApi`, with an isolated
        `dbus-run-session` service/client round trip.
  - [ ] **Remaining system adapters:** typed network, Bluetooth, and
        audio-device services; unavailable states remain intentional until
        those APIs exist.
- [ ] **Touchpad gestures (§13 / §4.4):** four-finger workspace switching etc.,
      gesture progress → UI progress
- [x] **Search & Launcher (§28) foundation:** private local application catalog,
      deterministic explainable ranking, and permission-gated typed launch
      execution. File/document/clipboard/command/calculator providers and the
      `Super+Space` desktop shortcut remain explicit follow-up adapters.
  - [x] **Gesture progress core:** interruptible/cancellable progress, velocity
        handoff, semantic workspace settling, and reduced-motion fixtures.
  - [ ] **Real input integration:** libinput gesture adapter, compositor IPC
        dispatch, and hardware/touchpad latency validation.
- [x] **Command / Action API (§21):** typed action + permission layer shared by
      search / automation / accessibility. **API contract accepted in
      ADR-0013:** caller-attributed action catalog, default-deny grants, trusted
      consent boundary, and audit are covered by deterministic fixtures;
      concrete portal/polkit/system-service adapters remain deferred.
- [x] **System overlay / popup contract:** renderer-neutral OSD, menu,
      popover, and modal/scrim roles now have typed output, anchor,
      exclusive-zone, input-region, focus, Escape/dismiss, accessibility, and
      token-motion contracts with deterministic SolUI fixtures (ADR-0015).
      Screen-recording and IME candidate-window product surfaces remain open.
- [x] **Layer-shell popup integration validation (ADR-0004 validation point #1):**
      repeatable headless compositor + SolUI fixture validates placement,
      fractional scale, input, focus, and lifecycle contracts. The existing
      `sol_session` test remains the real top-bar layer-shell round trip;
      native transient popup, physical multi-output, GPU, and AT-SPI validation
      remain field work rather than CI claims.

### M4 success criterion

> "SOL forms a complete, coherent desktop interaction model." Judged by: dock /
> launcher / overview / notifications / quick settings / touchpad gestures
> working under one animation + token system, with interruptible and reversible
> interactions.

> **Decision gate (PRD §41):** #10 global menu, #11 tiling product model
> (if not already settled), #16 search index, #17 System Action API. **#16 is
> settled by ADR-0014's local/private application catalog; #17 is settled at
> the API-contract level by ADR-0013. Production adapters and desktop-session
> validation remain follow-on work.**

---

## Phase 5 — Daily Driver

> **Goal:** let developers use SOL long-term as their primary desktop. **All
> "do-not-defer" themes settle here.**

### Milestone M5 deliverables

**Stability & hardware (hard settlement, §33)**

- [ ] **Suspend / Resume:** session restores correctly; surfaces/state intact
- [ ] **Multi-monitor:** hotplug, independent configuration, per-monitor
      workspaces
- [ ] **Fractional scaling:** crisp rendering at non-integer scales
- [ ] **NVIDIA:** driver path, private GBM / VRAM parameters
- [ ] **Touchpad / gestures:** mature gesture stack
- [ ] **Display hotplug:** complete
- [ ] **Audio / Bluetooth / Power** (PipeWire / BlueZ / UPower integration)

**Desktop core capabilities**

- [x] **Session-launch foundation:** installed `sol-session` validates an
      XDG runtime directory and deterministic socket name, starts
      `sol-compositor --tty-udev`, waits for its Wayland socket, then starts
      settingsd, notificationd, and portal session-bus services before
      `sol-shell` with the matching desktop/Wayland environment. The
      compositor remains session-critical while shell/service crashes restart
      independently. Dry-run and process supervision tests do not require DRM,
      a VT, or a login manager; `scripts/validate-session-services.sh` runs the
      real service daemons under an isolated session bus, probes all three
      names, and verifies shutdown releases them. Real
      display-manager, libseat/DRM, VT, and desktop-session validation remain
      required before calling this a field-validated session path.
- [ ] Clipboard, drag & drop fully polished
- [x] **Portal authorization foundation:** `sol-portal` maps typed document
      open and screen-capture requests through the caller-attributed
      `SystemActionApi`; default-deny and explicit authorization are fixture
      tested without granting arbitrary portal work.
  - [x] **Portal authorization D-Bus adapter:** `org.sol.Portal1` accepts only
        validated caller identity and document-open/screen-capture intents,
        returning decisions and correlation IDs without exporting executable
        authorization tokens. An isolated daemon/proxy test proves default
        deny and malformed-request rejection.
- [ ] **Screen sharing / screen recording:** XDG portal D-Bus, file chooser
      UI, PipeWire/screencopy adapters, stream lifecycle, and desktop-session
      validation remain required.
- [ ] Application compatibility matrix (GTK / Qt / SDL / Flutter / Electron —
      Wayland-native, §4.2)
- [ ] **IME complete (§21.1):** stable end-to-end flow for mainstream languages

**Security & diagnostics**

- [x] **Privacy-bounded diagnostics foundation:** `sol-diagnostics` records
      typed source/severity/code events with deterministic summary redaction,
      bounded retention, and private local storage; ADR-0016 prohibits shell
      access and opaque payloads at this boundary.
- [ ] **Live crash reporting:** authenticated service transport, real crash
      capture, consent UX, encrypted export/upload policy, and field validation
      remain required before treating diagnostics as a production reporter.
- [x] **Permission grant persistence foundation:** `FilePermissionStore`
      persists caller/capability allow or deny grants through the typed action
      boundary with atomic private files and repeatable revocation tests.
- [ ] **Production permission model (§31):** trusted consent UI, durable audit
      records, polkit/portal policy, and explicit authorization for recording,
      camera, microphone, location, secrets, and system settings remain.

### M5 success criterion

> "Developers can use SOL as their primary desktop environment long-term."
> Judged by: sustained stable use across daily scenarios (multi-monitor,
> external desktop or laptop, suspend/resume, NVIDIA or AMD), with IME and
> share/record available.

> **Production gate:** by the end of this Phase, installable packages for the
> official SOL Arch repos (`[sol-core]` / `[sol-apps]` / `[sol-sdk]`) should
> exist, and the `sol-desktop` meta package should install with a single
> `pacman -S` (PRD §7 / §30). **The Flatpak sandbox decision (§41 #12)
> settles here.**

---

## Phase 6 — Developer Platform

> **Goal:** build a third-party developer ecosystem and SDK stability promise
> (PRD §17, §23, §30, §42).

### Milestone M6 deliverables

- [ ] **Public SolKit:** public API polish and versioning
- [x] **SDK stability policy (§41 #8):** ADR-0017 defines post-v0.1 Public
      source-API semver gates, rejects a Rust ABI promise, and makes no claim
      that the current unpublished crates are stable.
- [ ] **Documentation:** getting started, guides, API reference
      (`sol-sdk-docs`)
- [x] **Starter template:** `templates/solkit-starter` provides a public-crate
      app skeleton, copy-out dependency instructions, and deterministic
      external-copy validation (`scripts/validate-solkit-starter.sh`)
- [x] **Project scaffolding:** `scripts/new-solkit-project.sh` creates a named
      external starter, validates its package and app identity, and is covered
      by `scripts/test-new-solkit-project.sh`
- [ ] **Templates:** `cargo new` / project scaffolding beyond the starter
- [ ] **Developer tools:** scaffolding, debugging, packaging tools
- [ ] **Packaging polish:** pacman/AUR integration, signed-repo trust chain
      (AUR not in the official trust chain, §30)
- [ ] **Store backend (§41 #15, optional):** hide package implementation
      details behind pacman/AUR
- [x] **SDK permission tiers (§23):** ADR-0017 formalizes Public, Restricted,
      and Private contracts and their dependency direction.
- [x] **Monorepo review (§39):** ADR-0017 retains the monorepo until a public
      SDK release, independent consumers, enforceable boundaries, and an
      independently versioned component justify a split.

### M6 success criterion

> "Third-party developers can build high-quality native apps without
> understanding SOL internals." Judged by: an external developer uses SolKit
> templates + docs to independently build and distribute an app into
> pacman/AUR.

> **Non-goals (PRD §37, at any phase):** self-built kernel / init / audio /
> network stacks, full app store, full AI assistant, mobile support, office
> suite, a full immutable OS, or early third-party SDK stability promises.

---

## Cross-cutting: technical debt & governance

| Topic | Starts | Notes |
|---|---|---|
| `sol-design` token convergence | Phase 2 | Every component must pass Design Review before entering `sol-ui` (§19.1 rule #2) |
| Consistency CI (golden snapshot) | Phase 2 | Turn "consistency" into a continuously verifiable mechanism (§19.1) |
| App identity format | Phase 2/3 | Prerequisite for launcher/commands/notifications/store (§41 #7) |
| Permission layer (typed action) | Phase 4 | Shared by search/automation/accessibility/AI (§21/§29) |
| Security model | Phase 4–6 | Reuse polkit + portal + Secret Service; sandbox settles in Phase 5 |
| Hardware test matrix | throughout | AMD → Intel → NVIDIA; laptop/desktop; single/multi-display; HiDPI (§33) |

## Long-term platform direction (after Phase 6, PRD §42)

```text
Linux
  ↓
SOL Desktop Runtime
  ↓
SolKit
  ↓
SOL Applications → Third-party Applications
```

- **Technical-asset focus:** Compositor + Desktop Runtime + Application
  Framework + Design System + First-party Applications.
- **Direction call:** users should not perceive underlying complexity because
  of Linux / Wayland / pacman — what they see is *SOL*.
- **Ongoing evaluation:** decoupling SOL Desktop and SOL OS by layer
  (PRD §3); long-term ARM64 support (§32).

---

## Revision history

- **2026-08-15** — Phase 1 M1 shell + IME milestones implemented and validated
  via CI. ADR-0006 accepts D-Bus for compositor↔shell IPC. layer-shell +
  text-input v3 + input-method v2 globals added to the compositor; sol-shell
  renders a top-bar layer surface and round-trips via a headless integration
  test; sol-ime frontend scaffold models the candidate window / preedit with
  sol-design tokens.

- **2026-08-16** — Phase 1 (Desktop Core) complete. All M1 deliverables
  achieved: window management (hit-test/focus/Alt+Tab, move/resize, Floating+Snap),
  workspace model, output management + HiDPI + display-hotplug, layer-shell
  protocol integration, D-Bus IPC decision, structural shell/compositor split,
  compositor text-input v3 + input-method v2 integration. Remaining items
  (fcitx5 transport wiring, DRM/udev multi-monitor) are post-M1 follow-on.
- **2026-08-16** — `sol-ime` gained an fcitx5 session-bus transport and typed
  engine/frontend event boundary. A fake pinyin round-trip covers preedit,
  candidates, and commit without a daemon; the optional D-Bus smoke verifies
  input-context setup and frontend UI signals on a running fcitx5 session.
