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
| 1 | Desktop Core | A usable daily Wayland compositor | SOL works as a basic daily-use Wayland compositor | ⏳ Next up (in progress) |
| 2 | SolKit | Native app framework | Build an app with native SOL look and interaction entirely in SolKit | ⏸ Not started |
| 3 | First-party Applications | First-party apps | The three first-party apps share a unified UX | ⏸ Not started |
| 4 | Shell Experience | Complete desktop interaction model | SOL forms a complete, coherent desktop interaction model | ⏸ Not started |
| 5 | Daily Driver | Long-term daily use | Developers can use SOL as their primary desktop long-term | ⏸ Not started |
| 6 | Developer Platform | Ecosystem & SDK stability | Third-party devs build high-quality native apps without knowing SOL internals | ⏸ Not started |

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

### Phase 0 → Phase 1 handoff

- **Reusable core:** `SolState` (`compositor/src/state.rs`) is decoupled from
  the backend. In Phase 1 it keeps growing; do not bloat `main.rs`.
- **Known carryovers:** the DRM/udev backend requires `libdisplay-info < 0.3.0`
  and must be enabled on real hardware (ADR-0005); the shell IPC wire format
  is not committed (ADR-0006); Shell/SDK/Apps are still scaffolds.
- **Reference implementation:** Smithay `anvil` example (vendored
  `smithay-0.7.0/anvil/`) = the model for real hit-testing / focus /
  DRM-udev wiring.

---

## Phase 1 — Desktop Core

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
- [ ] Tiling / snapping foundation: Floating + Snap (PRD §12); Tiling optional
      (§41 #11 undecided)
- [x] Keyboard focus switching (Alt+Tab to start); correct
      `Activated`/`Unactivated` delivery

**Workspace & Output**

- [ ] Workspace model (PRD §13): surface groups + switching; reserve a
      progress interface for touchpad interactive transitions (§4.4)
- [ ] Output management / basic multi-monitor (enumerate and map multiple
      outputs; do not defer to the end, §33)
- [ ] Clear-screen / HiDPI basics (`scale` pass-through; fractional scaling
      verified in Phase 5)
- [ ] Basic display-hotplug handling (§33)

**Shell integration & IPC**

- [x] `layer-shell` protocol integration (prerequisite for the shell top bar /
      dock; a key ADR-0004 validation point)
- [x] **Typed IPC decision:** D-Bus chosen among ADR-0006's three options
      (D-Bus / custom Wayland protocol / shm ring) — ADR-0006 accepted
- [x] First shell surface: `sol-shell` renders as a layer-shell top bar
      (validated by the `sol_session` shell round-trip integration test)
- [ ] Structural guarantee: a shell crash must not kill the compositor
      (PRD §11 hard constraint); shell is already a separate process over
      layer-shell + D-Bus, so the compositor is insulated by construction

**Input method (IME) — first class (PRD §21.1 / §40)**

- [x] Compositor-side `text-input v3` + `input-method v2` protocol integration
      (note: Smithay 0.7 ships text-input v3 + input-method v2 — the newer v4 /
      v3 staging protocols are evaluated when Smithay raises them; ADR-0007)
- [x] `sol-ime` frontend scaffold: candidate window / preedit data model +
      layout with `sol-design` tokens (visual rendering via `sol-ui` lands in
      Phase 2, when `sol-ui` exists)
- [ ] fcitx5 engine bridge (`fcitx5-ime` / `fcitx5-chinese-addons`; Chinese
      pinyin first)

### M1 success criterion

> "SOL can be used as a basic daily-use Wayland compositor."
> Judged by: windows can be created/moved/resized/focused, multiple
> workspaces switchable, multi-monitor works, the shell top bar coexists with
> the compositor over the settled IPC, and Chinese pinyin input works.

### M1 dependencies & risks

| Dependency | Notes |
|---|---|
| `layer-shell` protocol | Requires introducing layer-shell from `wayland-protocols` (confirm no wlr-protocols clash) |
| DRM/udev backend | Real-hardware session (resolve the ADR-0005 `libdisplay-info` issue on target hardware) |
| IPC transport | Must be settled before the first shell surface (ADR-0006) |

---

## Phase 2 — SolKit

> **Goal:** form a complete **native application development framework** so
> third-party developers can build apps with SOL-native look and interaction
> (PRD §17–23).

### Milestone M2 deliverables

**SolUI & rendering architecture**

- [ ] **Decision #1 (ADR-0004 validation):** complete the Slint/SolUI spike
      and settle the SolUI rendering architecture (retained vs declarative,
      Slint-backed or not)
- [ ] Semantic component system: `Button` / `Toolbar` / `Dialog` / `List` /
      `MenuItem` / `TextField` / `Tab` … (PRD §18); apps write intent, not
      visuals
- [ ] Layout engine (`HStack` / `VStack` semantic layout, PRD §18)
- [ ] `sol-design` full token convergence: typography / spacing / radius /
      material / motion / shadows / color (PRD §19, §19.1)
- [ ] Consistency testing: golden-snapshot asserts component-tree output
      contains only token values (§19.1)

**SolAnimation**

- [ ] Unified animation engine (PRD §14): easing / spring / interactive
      progress / velocity / interruption / reversal
- [ ] Semantic motion tiers: `Motion::Control/Panel/Window/Workspace`
- [ ] One set of animation semantics shared by compositor and UI

**SolApp & lifecycle**

- [ ] Application lifecycle / window lifecycle / session restoration
      (PRD §20)
- [ ] Command architecture (PRD §21): `file.open` / `edit.copy` … auto-exposed
      to menus / shortcuts / command palette
- [ ] SolGraphics abstraction, SolSystem, SolAccessibility scaffold
      (§35 — accessibility enters the architecture early)

**Keyboard / accessibility / theme**

- [ ] Standard keyboard interaction & focus management implemented uniformly
      by `sol-ui` (§19.1 behavior consistency)
- [ ] Accessibility semantic tree, reduced motion, high contrast (§35)
- [ ] Theme switching touches only `sol-design` (§19.1 single source of truth)

### M2 success criterion

> "It is possible to fully develop an app with SOL-native look and
> interaction using SolKit." Judged by: an app with layout / components /
> commands / animation / keyboard navigation written entirely with `solkit`
> (`sol-app` + `sol-ui` + `sol-design` + `sol-animation`), without touching a
> concrete renderer.

> **Decision gate (PRD §41):** this Phase settles #1 SolUI rendering
> architecture, #2 retained/declarative, #6 settings storage architecture,
> #7 application identity format.

---

## Phase 3 — First-party Applications

> **Goal:** ship three first-party apps (Files / Terminal / Settings) and
> dogfood SolKit (PRD §24–27).

### Milestone M3 deliverables

- [ ] **Settings (PRD §26):** Appearance / Displays / Sound / Network /
      Bluetooth / Keyboard / Mouse / Touchpad / Power / etc.; layered as
      UI → Settings API → system services
- [ ] **Terminal (PRD §27):** GPU-accelerated rendering, tabs, Unicode,
      true color, clipboard, search, configurable shell
- [ ] **Files (PRD §25):** sidebar / tabs / search / drag & drop / preview /
      removable storage / network locations / context actions / trash /
      keyboard navigation — the **dogfooding baseline (§19.1)**
- [ ] Dogfooding loop (§24): framework gap found in an app → improve SolKit →
      all apps benefit
- [ ] Command palette / keyboard navigation consistent across the three apps

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

- [ ] **Dock / Launcher:** app launch, running indicators, pin / unpin
- [ ] **Overview / Workspace:** workspace overview, visual switching
- [ ] **Top Bar:** clock, status area (network / volume / battery / bluetooth)
- [ ] **Notification Center** (wired to `sol-notificationd`)
- [ ] **Quick Settings** (wired to `sol-settingsd`)
- [ ] **Touchpad gestures (§13 / §4.4):** four-finger workspace switching etc.,
      gesture progress → UI progress
- [ ] **Search & Launcher (§28):** default `Super+Space`; Applications / Files /
      Settings / Commands / Calculator to start
- [ ] **Command / Action API (§21):** typed action + permission layer shared by
      search / automation / accessibility
- [ ] **System overlays:** screen-recording indicator, IME candidate window, etc.
- [ ] Layer-shell popup integration validation (ADR-0004 validation point #1)

### M4 success criterion

> "SOL forms a complete, coherent desktop interaction model." Judged by: dock /
> launcher / overview / notifications / quick settings / touchpad gestures
> working under one animation + token system, with interruptible and reversible
> interactions.

> **Decision gate (PRD §41):** #10 global menu, #11 tiling product model
> (if not already settled), #16 search index, #17 System Action API.

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

- [ ] Clipboard, drag & drop fully polished
- [ ] Screen sharing / screen recording (portal + screencopy)
- [ ] Application compatibility matrix (GTK / Qt / SDL / Flutter / Electron —
      Wayland-native, §4.2)
- [ ] **IME complete (§21.1):** stable end-to-end flow for mainstream languages

**Security & diagnostics**

- [ ] Diagnostics / crash-reporting architecture (§41 #18; must not require
      arbitrary shell access)
- [ ] Permission model (§31): sensitive capabilities explicitly authorized
      (screen recording / camera / microphone / location / secrets / system
      settings)

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
- [ ] **SDK stability strategy (§41 #8):** introduce an ABI/API stability
      promise after v0.1
- [ ] **Documentation:** getting started, guides, API reference
      (`sol-sdk-docs`)
- [ ] **Templates:** `cargo new` / project scaffolding, app skeleton
- [ ] **Developer tools:** scaffolding, debugging, packaging tools
- [ ] **Packaging polish:** pacman/AUR integration, signed-repo trust chain
      (AUR not in the official trust chain, §30)
- [ ] **Store backend (§41 #15, optional):** hide package implementation
      details behind pacman/AUR
- [ ] Formalize the third-party SDK permission tiers: Public / Restricted /
      Private (§23)
- [ ] Re-evaluate monorepo boundaries (§39: split after API stabilization?)

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
  sol-design tokens. Initial roadmap published based on the seven PRD §38
  phases, mapped against the current repository state and settled ADRs. Each
  Phase's section is refined as it closes.
