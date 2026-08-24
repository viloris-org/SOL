# SOL Operating System — Product Requirements Document

**Version:** v0.2
**Status:** Concept / Pre-Alpha
**Platform:** SOL OS / Linux kernel
**Project name:** SOL
**Core technology stack:** Rust + Wayland + Smithay

---

# 1. Product overview

SOL is a modern, application-first operating system built on the Linux kernel.

SOL's goal is not to produce a Linux theme that mimics the look of macOS, nor is
it a simple fork of GNOME, KDE Plasma, or any other existing desktop environment.
Instead, SOL owns the complete product boundary from verified boot through the
application framework and graphical experience.

SOL will own its own:

- Bootloader, recovery, and system-image lifecycle
- Package manager and signed `.app` bundle format
- Application identity, sandbox, and atomic permission control
- System-managed accounts and encrypted credential storage
- Wayland compositor
- Desktop shell
- Application SDK
- Design system
- Adaptive SOL fluid material language
- System services
- First-party applications
- Application-distribution experience

The implementation continues to leverage mature Linux infrastructure,
including the Linux kernel, systemd, PipeWire, NetworkManager, BlueZ, Mesa,
udisks2, and others. Reuse does not make the host distribution SOL's public
product or compatibility boundary.

SOL's core principle is:

> Reuse Linux. Own the operating-system contract.

---

# 2. Product vision

SOL aims to deliver a complete Linux-kernel operating system that meets the
quality bar of modern commercial desktop operating systems.

Key goals include:

- Highly consistent system UX
- High-performance, low-latency graphics experience
- Complete touchpad and gesture interaction
- Excellent HiDPI and multi-monitor support
- Cohesive first-party application experience
- Full application-development SDK
- Verified boot, recovery, and transactional rollback
- Dependency-isolated, self-contained application bundles
- A coherent, default-deny application permission model
- System-managed account and credential lifecycle
- A coherent, accessible fluid-glass material system
- Compatibility with the broader Linux application ecosystem
- Remaining open to power users while preserving core Linux flexibility
- Hiding unnecessary Linux system complexity from casual users

SOL does not treat "highly customizable" as a primary goal.

Rather than letting each component have a radically different look and
behavior, SOL emphasizes **coherent, predictable, and polished** behavior.

---

# 3. Product positioning

SOL is:

> A modern, application-first operating system built on the Linux kernel.

SOL is not a desktop environment layered onto an arbitrary Arch installation.
Arch may be used as an early build and bootstrap source, but the installed
system is built, updated, secured, and recovered through SOL-owned contracts.

The relationship is:

```text
Linux kernel + upstream components
      ↓
SOL boot / system image / update authority
      ↓
SOL security + package + system services
      ↓
SOL Runtime / compositor / shell
      ↓
SOL and third-party .app applications
```

The former “SOL Desktop installable on Arch” direction is superseded. Desktop
components remain modular internally, but SOL OS is the shipped product.

---

# 4. Product principles

## 4.1 Consistency First

First-party apps must share a single, unified:

- Typography
- Layout
- Spacing
- Navigation
- Window behavior
- Animation
- Context menu
- Keyboard interaction
- Accessibility
- System integration

Apps must not re-implement these interaction patterns independently.

---

## 4.2 Wayland Native

SOL uses Wayland as the sole first-class graphics protocol.

SOL does not ship an independent X11 session.

Traditional X11 applications are not a compatibility target. SOL focuses on the
modern Linux application ecosystem: GTK, Qt, SDL, Wayland-native, Electron,
and Flutter apps.

---

## 4.3 Framework First

SOL's system consistency comes from the Application Framework, not from
developer-convention documents.

Correct behavior should be provided by the SDK by default.

```text
Application
    ↓
SolKit
    ↓
SOL Design System
    ↓
SOL Runtime
```

rather than requiring every developer to hand-copy system behavior.

---

## 4.4 Interactive Motion

Animation is not visual decoration — it is part of SOL's interaction model.

The system prioritizes:

- Interruptible animations
- Interactive transitions
- Spring animations
- Gesture-driven transitions

Touchpad operations should aim to produce:

```text
Finger movement
      ↓
Gesture progress
      ↓
UI progress
```

not:

```text
Gesture detected
      ↓
Play animation
```

---

## 4.5 Linux Compatibility

SOL does not build a closed application ecosystem.

The system supports mainstream Linux application technologies:

- GTK
- Qt
- SDL
- Electron
- Flutter
- Native Wayland applications packaged as `.app`
- Flatpak through an optional compatibility subsystem

First-party apps prioritize SolKit.

Compatibility has three support levels:

```text
Native      SolKit/SolUI                     full SOL UX and framework contract
Integrated  GTK/Qt + official SOL adapter   full capabilities + mapped system UX
Compatible  generic Wayland .app            standard Wayland/portal behavior
```

All levels use the same `.app` identity, sandbox, explicit atomic permissions,
system-managed accounts, update, and rollback model. GTK/Qt/Electron/Flutter/
SDL runtimes and their plugins are private bundle dependencies; SOL never
replaces them with an arbitrary host copy or globally injected theme engine.

## 4.6 Transactional and Recoverable

System and application activation is atomic. A failed or interrupted update
must leave the last known-good version usable. Boot and recovery understand the
same signed system-image state as the package manager.

## 4.7 Self-contained Apps, Shared Platform

Every `.app` contains its non-SOL dependencies. No application may rely on an
arbitrary library from the mutable host filesystem. Common SOL capabilities are
provided by versioned runtime slots with stable ABI or IPC contracts so apps do
not need to bundle the entire platform runtime.

## 4.8 Least Authority

Third-party applications run in a default-deny sandbox. Declaring a capability
only makes it requestable; every protected use requires an explicit user or
managed-policy grant. Grants use the smallest resource, operation, and duration
that can complete the action. Grant + audit + handle issuance is atomic, and
unrelated permissions are never bundled behind one “Allow all” decision.
Protected resources are accessed through typed APIs, portals, and brokers.

## 4.9 System-managed Accounts

The OS owns local users, connected accounts, provider scopes, credentials,
recovery, and revocation. Applications receive opaque account handles and
short-lived scoped leases only after explicit authorization; they do not own
durable passwords, refresh tokens, or the system account database.

## 4.10 Fluid Material, Accessible by Construction

SOL uses its own adaptive translucent material language for functional chrome,
navigation, controls, and elevated surfaces. Dense content remains solid.
Material hierarchy, contrast, motion, and degradation are system-resolved;
reduced transparency and high contrast always have solid alternatives.

---

# 5. System architecture overview

```text
┌──────────────────────────────────────────────┐
│                 Applications                 │
│                                              │
│ Files / Terminal / Settings / Store / ...   │
├──────────────────────────────────────────────┤
│                    SolKit                    │
│                                              │
│ UI / App / Commands / Documents / System    │
├──────────────────────────────────────────────┤
│             SOL Desktop Runtime              │
│                                              │
│ Search / Notifications / Settings / Portal  │
├──────────────────────────────────────────────┤
│                  SOL Shell                   │
│                                              │
│ Dock / Launcher / Overview / System UI       │
├──────────────────────────────────────────────┤
│               SOL Compositor                 │
│                                              │
│ Smithay / Wayland / Scene / WM / Input      │
├──────────────────────────────────────────────┤
│       SOL OS Services / Security / Packages  │
│                                              │
│ sol-securityd / sol-packaged / portals      │
├──────────────────────────────────────────────┤
│        SOL System Image and Boot Chain       │
│                                              │
│ sol-boot / Linux / systemd / Mesa / etc.    │
└──────────────────────────────────────────────┘
```

---

# 6. Base system and boot

SOL builds a reproducible, signed system image around the Linux kernel. Arch
may supply early upstream packages and the development environment, but SOL
owns image composition, release channels, update policy, compatibility, and
recovery.

Priority upstream reuses:

```text
Linux Kernel
systemd
udev
Mesa
PipeWire
WirePlumber
NetworkManager
BlueZ
polkit
udisks2
UPower
Linux namespaces / cgroups / seccomp / LSMs
UEFI and UKI conventions
```

The native boot path is `UEFI → sol-boot → verified A/B system deployment →
Linux userspace`. Each deployment binds its kernel, initrd, root-image digest,
runtime descriptors, and generation. `sol-boot` owns signature policy, slot
selection, bounded retry, known-good fallback, and recovery handoff. Current and
fallback `sol-boot` copies and recovery copies remain independently addressable;
their updates use inactive-copy verification and one-shot trial activation rather
than overwriting the only known-good path. User data remains outside system
slots.

---

# 7. Package architecture

SOL owns `sol-pkg` (CLI and inspection) and `sol-packaged` (privileged
transaction service). A Software app is a client of this authority, not a
parallel installer.

Boot/recovery copies use verified one-shot trial activation. System releases are
signed, read-only deployments activated through A/B boot slots.
Applications are signed `.app` bundles installed in a content-addressed store
and activated by an atomic version switch. Both use the transaction sequence:

```text
resolve → fetch → verify → stage → validate → commit
```

A `.app` contains a canonical manifest, signatures, executable entry points,
private libraries, resources, extensions, SBOM, licenses, and provenance. It
vendors every non-SOL userspace dependency and has no root install scripts.
Applications may depend on a named, side-by-side SOL Runtime major such as
`sol-runtime-1`, plus a minimum contract revision and required stable feature
set, but never on arbitrary system-image libraries. An app transaction sets a
preferred signed hash and an ordered fallback chain of
verified, previously activated hashes; launch selects the first non-revoked
compatible hash as the effective version. Display version strings are not used
for ordering. OS rollback never rewrites the preferred pointer. If no compatible
hash exists, that app is explicitly unavailable without blocking boot or
changing its data. App updates prepend to the chain; explicit app rollback
truncates newer resolution candidates; reinstall starts a fresh chain.

The normative bundle and transaction contracts live in the
[OS Platform Definition](os-platform.md).

---

# 8. Update model

Upstream Linux components flow through SOL-owned integration and release gates.

The recommended update path:

```text
Upstream projects / build inputs
      ↓
SOL Integration
      ↓
Reproducible image + hardware/security testing
      ↓
Signed SOL channel
      ↓
Inactive A/B slot
      ↓
Boot validation → commit or automatic rollback
```

Desktop updates must pass a basic integration-test gate.

Key tested layers:

- Kernel
- Mesa
- systemd
- Wayland
- PipeWire
- NVIDIA drivers
- AMD drivers
- Intel graphics

Atomic image updates, boot fallback, and recovery are now OS release
requirements rather than optional long-term work.

---

# 9. SOL Compositor

## 9.1 Tech stack

Core language:

```text
Rust
```

Wayland compositor framework:

```text
Smithay
```

SOL Compositor does not re-implement Wayland, DRM/KMS, and the input stack
from scratch. Smithay provides the foundational compositor building blocks;
SOL builds on top:

- Window model
- Scene model
- Workspace model
- Animation
- Window management
- Gesture behavior
- Shell integration

---

# 10. Compositor architecture

```text
sol-compositor
├── backend/
│   ├── drm/
│   ├── input/
│   ├── session/
│   └── devices/
│
├── protocol/
│   ├── xdg-shell/
│   ├── layer-shell/
│   ├── output/
│   └── screencopy/
│
├── scene/
│   ├── surface/
│   ├── window/
│   ├── layer/
│   └── effects/
│
├── wm/
│   ├── focus/
│   ├── placement/
│   ├── workspace/
│   ├── floating/
│   └── tiling/
│
├── input/
│   ├── keyboard/
│   ├── pointer/
│   ├── touch/
│   └── gestures/
│
├── animation/
├── renderer/
└── shell-ipc/
```

---

# 11. Shell architecture

The compositor and the shell must not be fused into one monolith.

Preferred arrangement:

```text
sol-compositor
       ↕
   Typed IPC
       ↕
sol-shell
```

SOL Shell owns:

- Top bar
- Dock
- Launcher
- Workspace overview
- Notification center
- Quick settings
- System overlays
- Desktop UI

The Shell follows a fixed spatial grammar:

```text
upper left   authenticated foreground app identity + global menu
upper right  Live Capsule + application status + notifications + system status
bottom       centered SOL Dock + Application Launcher entry
window left  Close + Minimize + Maximize/Restore controls
```

Application navigation sidebars use the SOL `Sidebar` fluid material while
dense application content remains solid. The complete placement, trust,
multi-display, and accessibility rules live in the
[Shell Spatial and Live Activity Contract](shell-experience.md).

The foreground menu is a Shell-rendered atomic command snapshot bound to the
compositor-authenticated focused App ID. The upper-right application tray is a
typed status registry, not arbitrary embedded client windows.

Live Capsule is a trusted Shell surface for ongoing, immediately actionable
state such as recording, calls, timers, transfers, microphone/camera use, screen
sharing, and remote control. Apps register attributed declarative activities
through a leased API after explicit `shell.live-activity` authorization.
Microphone/camera/capture privacy activities come from the capability broker,
cannot be hidden by apps, and expose system-owned Stop/Revoke controls.

The compositor owns:

- Wayland surfaces
- Windows
- Input
- Focus
- Workspaces
- Outputs
- Frame scheduling
- Scene composition

A shell crash must not force the compositor to exit.

---

# 12. Window management

SOL defaults to floating window management.

Modern snap-and optional advanced tiling are also supported.

Model:

```text
Floating
   +
Snap
   +
Optional tiling
```

Average users do not need to understand the tiling-window-manager concept.
Power users can enable more sophisticated window-layout capabilities.

SOL-native window controls live at the physical upper-left of each window in
`Close, Minimize, Maximize/Restore` order. Server-side decorations use this
layout. GTK/Qt adapters request it through supported public APIs; generic
client-decorated Wayland applications may retain their own controls rather than
being patched through private toolkit internals.

---

# 13. Workspace

SOL provides a continuous, low-latency workspace system.

Workspace switching must support interactive touchpad transitions.

For example:

```text
Workspace A ←──────→ Workspace B
              ↑
        finger position
```

Workspace animations must be interruptible, reversible, cancellable, and
continuable.

---

# 14. Animation system

SOL Compositor includes a unified animation engine.

Basic model:

```text
Current State
      ↓
Animation
      ↓
Target State
```

Supports:

- easing
- spring physics
- interactive progress
- velocity
- interruption
- reversal

Animation semantics should be abstracted as:

```text
Control
Panel
Window
Workspace
Interactive
```

rather than requiring callers to specify raw millisecond values.

---

# 15. Renderer

The renderer is decoupled from the window manager and the animation engine.

Conceptual interface:

```text
Renderer
├── Surface
├── Texture
├── Shadow
├── Blur
├── Transform
├── Color
└── Present
```

The first phase reuses Smithay's existing renderer capabilities.

Vulkan / wgpu / custom rendering pipelines are evaluated later per need.

No renderer rewrite for the sake of technological novelty.

---

# 16. Graphics requirements

SOL's long-term graphics targets include:

- 60 Hz
- 90 Hz
- 120 Hz
- 144 Hz+
- Variable refresh rate
- Fractional scaling
- HiDPI
- Multi-monitor
- Direct scanout
- Damage tracking
- Color management
- HDR
- Low-latency input
- Correct frame scheduling

MVP does not require all advanced capabilities at once; the architecture must
not block them from landing later.

---

# 17. SolKit

SolKit is SOL's source SDK and application framework. The SOL Framework Runtime
is its installed, versioned platform counterpart.

SolKit is not merely a widget toolkit.

Third-party applications declare a runtime major (for example,
`sol-runtime-1`), a minimum monotonically increasing contract revision, and any
required stable feature names. Compatible updates may advance the revision and
add features but cannot remove older ones; breaking changes install as a new
side-by-side major. Apps do not bind to an unstable Rust ABI: in-process
boundaries use a stable C-compatible ABI where required, while system
capabilities use versioned IPC. SolKit provides safe language bindings.

This is the exception to `.app` dependency self-containment: an app vendors all
non-SOL dependencies but can share the stable SOL platform. It can therefore
ship less runtime without inheriting dependency resolution from the base image.

Overall structure:

```text
SolKit
├── SolUI
├── SolApp
├── SolGraphics
├── SolAnimation
├── SolWindow
├── SolCommands
├── SolDocuments
├── SolStorage
├── SolSystem
├── SolAccessibility
└── SolTesting
```

---

# 18. SolUI

SolUI provides SOL-native UI.

Responsibilities:

- Layout
- Components
- Typography
- Theme
- Input
- Focus
- Animation
- Accessibility
- Rendering integration

UI should use semantic APIs by default.

Preferred:

```text
Button
    role: Primary
```

not:

```text
Button
    radius: 8
    padding: 12
    background: #...
```

Visual properties are controlled by the design system.

---

# 19. Design tokens

SOL establishes a unified design-token system.

Categories:

```text
Typography
Spacing
Radius
Color
Material
Shadow
Motion
Iconography
```

SOL's material tokens define semantic `Content`, `Chrome`, `Panel`, `Floating`,
`Control`, `Sidebar`, `Dock`, and `Capsule` roles. Apps never choose raw blur,
translucency, refraction, saturation, or specular values. The
renderer/compositor resolves materials for theme, backdrop, accessibility,
power, and GPU capability without exposing backdrop pixels to the app.
Repeated light-on-light glass nesting is forbidden; surfaces consolidate or
become solid before legibility degrades.

First-party apps must not copy hard-coded visual parameters.

Preferred:

```text
Motion::Fast
Motion::Control
Motion::Panel
Motion::Window
Motion::Workspace
```

not:

```text
animation_duration = 217ms
```

---

## 19.1 Mechanism for enforcing UI consistency

UI consistency is SOL's first principle (§4.1), but it cannot be maintained by
"developer convention" alone. **Consistency must be enforced by architecture,
not by discipline.**

**Core principle: a single source of truth for visual parameters.**

Only the Design Token crate (`sol-design`, described in this section) may
define concrete visual parameters (color, spacing, radius, duration, curve,
font size, …). UI components and first-party apps reference these tokens by
name — never a bare `#RRGGBB`, bare `8.0`, or bare `217ms`. Type-safe wrapper
types turn "wrong usage" into a **compile error** rather than a style drift.
Consistency is therefore guaranteed by the type system, not by convention.

### Single source of truth

- Tokens live in exactly one place: the `sol-design` crate.
- UI components receive token references, never raw color / numeric values.
- Theme / skin switching touches only `sol-design`.

Example (type-safe):

```text
Button
    shape: Shape::Primary       // not radius: f32
HStack
    spacing: Spacing::Md        // not padding: 12
Color::Surface                  // not #hex
Motion::Panel                   // not duration = 170ms
```

### Minimal token set for v0.1

```text
sol-design
├── color/      semantic colors (Surface/Elevated/Accent/Text/Border/Error…)
├── typography/ named sizes (Body/Title/Label/Display + weight)
├── spacing/    spacing scale (Xs/Sm/Md/Lg/Xl)
├── radius/     corner-radius scale (None/Sm/Md/Full)
├── material/   surface hierarchy (Base/Panel/Floating → shadow/blur)
├── motion/     motion tiers (Fast/Panel/Window/Workspace → duration+curve)
└── shadows/    shadow specs
```

### Semantic component tree

App code should never contain concrete visual parameters. Apps only write
"what it is", never "how it looks":

```text
solui::Toolbar
    solui::Button(role: .Primary, label: "Open")
    solui::ToolbarSeparator
```

The consistency cost is borne once by `sol-ui`, not duplicated across every
app. All first-party apps (`sol-files` / `sol-terminal` / `sol-settings`) are
built via `solui` components + `sol-design` tokens, and do not operate directly
on the rendering layer.

### Behavior consistency (beyond visuals)

- Context menu, toolbar, tab, dialog, text field, list, nav — these
  interaction components are provided by `sol-ui`. Apps must not invent their
  own context menus / interaction components. Behavior splits are as damaging
  as visual splits.
- Standard keyboard interaction and focus management are implemented
  uniformly by `sol-ui`; apps do not re-implement them.

### Design review — iron rules

These rules act as a mandatory merge gate:

1. Any `sol-ui` / first-party-app commit that contains **bare hex, bare ms,
   or bare f32 visual parameters** is rejected at review time.
2. Every new component must pass Design Review before entering `sol-ui`,
   confirming that all its visual parameters are resolved from `sol-design`
   tokens.
3. `sol-files`, as the most complex first-party app, carries the dogfooding
   baseline: new components are first polished in `sol-files`, then sink back
   into `sol-ui` for all first-party apps to share (PRD §24 / §25 dogfooding
   loop).

### Consistency tests

Consistency can be tested — it should not be left to human review alone:

- Golden-snapshot testing: rendered output may contain only values from the
  token table. Traverse the component tree and assert that no non-token
  values appear.
- Consistency becomes a CI check, turning "consistency" from a slogan into a
  sustainably verifiable mechanism.

### Acceptable inconsistency boundaries

First-party `sol-ui` apps can be architecturally guaranteed to be consistent.
Third-party apps (GTK / Qt / Electron / Flutter) do not share `sol-ui` or
`sol-design`, and cannot be guaranteed to reach the same control-level visual
consistency. This visual boundary does not reduce their system capabilities.

SOL treats the two consistency goals separately:

```text
SolKit apps                   → architecturally enforced component consistency
GTK/Qt with SOL adapter       → mapped appearance, accessibility, windowing,
                                portals, accounts, and semantic materials
Generic Wayland .app          → baseline Wayland/portal compatibility
```

Official `sol-gtk` and `sol-qt` adapters are bundled with the application at a
toolkit-compatible version and talk to stable SOL ABI/IPC. They may map tokens
through supported GTK/Qt theme and style APIs, but must not patch private
toolkit internals or inject process-global modules. A constrained compositor
protocol may accept semantic material roles without returning backdrop pixels
to the client. Pixel-identical SolUI widgets are not promised outside SolUI.

The design phase must make clear: SOL does not demand that third-party
GTK/Qt apps "look like SOL" — that is outside the platform's controllable
scope.

---

# 20. SolApp

SolApp owns application lifecycle.

Core abstractions include:

```text
Application
Scene
Window
View
Command
Document
Task
SystemService
```

The application framework provides:

- App lifecycle
- Window lifecycle
- Session restoration
- Commands
- Menus
- Keyboard shortcuts
- Clipboard
- Drag & drop
- Notifications
- Recent documents
- State restoration

---

# 21. Command architecture

SOL establishes a unified command system.

Examples:

```text
file.new
file.open
file.save
window.close
window.minimize
window.move_left
edit.copy
edit.paste
```

The same command is automatically exposed to:

```text
Menu
Context menu
Keyboard shortcut
Command palette
Accessibility
Search
Automation
```

Future system-intelligence capabilities should call on system functions
through the Command / Action API rather than simulating mouse and keyboard.

---

## 21.1 Input method (IME)

Input method quality is one of the few system-level factors that shapes the
"everyday desktop" experience; it cannot be deferred to Phase 5/6.

**SOL decision: Option A — first-party IME frontend + reuse fcitx5 as the
engine backend.**

```text
sol-compositor
     │ target: text-input v4 / input-method v3 protocols
     ▼
sol-ime   (first-party frontend + candidate-window/preedit model; sol-ui rendering pending)
     │            ↘ engine: reuse fcitx5
     ▼
fcitx5-ime / fcitx5-chinese-addons (pinyin and other mainstream language engines)
```

- `sol-ime` owns the first-party IME frontend and candidate-window/preedit
  model. Candidate-window rendering with `sol-ui` and the fcitx5 transport are
  follow-on work.
- The protocol target is `text-input v4` + `input-method v3`. The current
  Smithay 0.7 implementation advertises and dispatches `text-input v3` +
  `input-method v2`; SOL will evaluate the newer staging protocols when
  Smithay supports them. This protocol integration remains a Phase 1 concern,
  not Phase 5/6.
- **We do not self-host a pinyin engine**: pinyin segmentation / candidate
  ranking is a decade-scale accumulation (`libpinyin` / `rime` / `fcitx5`
  among others). SOL reuses `fcitx5` addon engines, starting with Chinese
  pinyin.

### Language priorities

- v0.1: **mainstream languages first** (Chinese pinyin, etc.), backed by
  fcitx5 addons (`fcitx5-chinese-addons` etc., already present on Arch).
- Extend to Japanese (Anthy/KKC), Korean (Hangul), and others per fcitx5
  addon support.

### Why not Option C (self-hosted engine)

SOL's core asset is compositor + SDK + first-party experience. **We should
not burn compute on "building an IME engine"**: engine quality is extremely
hard to catch up to against a decade of community accumulation, and it does
not produce a differentiating desktop experience.

### Why not skip IME entirely

IME is not an X11 legacy issue (dropping XWayland is the right call); it is
part of "consistent experience across SOL's app ecosystem." It belongs as a
first-class citizen in the compositor / sol-ui / sol-design stack, not as a
late-stage bolt-on.

### Contextual candidate ranking proposal

SOL may add a local contextual and personalized candidate-ranking layer while
keeping fcitx5 authoritative for composition, segmentation, conversion, and
candidate validity. This work is gated by a traditional-first, fail-open MVP
and measured shadow-mode improvement; a custom language engine is not implied.
See the [SOL Contextual IME PRD](contextual-ime-prd.md) for scope, privacy,
performance, experiment, and launch requirements.

---

# 22. Document architecture

SolKit provides a standard document model.

Supports:

- Open
- Save
- Save as
- Autosave
- Revert
- Recent documents
- Session restore
- Unsaved-state tracking

Intended to serve:

```text
Text editor
Image viewer
PDF viewer
IDE
Office applications
Creative applications
```

---

# 23. SDK permission tiers

SOL distinguishes:

```text
Public SDK
Restricted system SDK
Private shell SDK
```

First-party normal apps prefer dogfooding the Public SDK. Only components
that truly touch system permissions should use restricted / private APIs.

Example:

```text
SolUI             Public
SolApp            Public
SolDocuments      Public
SolGraphics       Public

SolSystem         Restricted
SolShellKit       Private
SolSecurityKit    Restricted
SolAccounts       Restricted
SolCompat         Public toolkit/portal adapter contracts
```

Goal: third-party developers can produce apps nearly as polished as
first-party apps — there should not be an artificial first-party API
advantage.

SDK visibility does not imply authority. Public APIs operate within the app's
sandbox. Restricted APIs issue typed requests to system brokers and remain
subject to declaration, policy, user consent, and audit.

---

# 24. First-party applications

SOL's consistency requirements demand a set of high-quality first-party apps.

MVP:

```text
Files
Terminal
Settings
```

After MVP:

```text
Software / Store
Image viewer
PDF viewer
Text editor
System monitor
Archive manager
Calculator
Screenshot tool
Screen recording
```

First-party apps also carry the responsibility of dogfooding SolKit.

Rule:

> When multiple apps need the same system-level interaction, improve SolKit
> first — do not hand-roll a workaround in each app separately.

Development loop:

```text
SolKit
  ↓
First-party App
  ↓
Framework limitation discovered
  ↓
Improve SolKit
  ↓
All applications benefit
```

---

# 25. Files

Files is one of SOL's most important first-party apps.

Core requirements:

- Native SolKit UI
- Sidebar
- Tabs
- Search
- Drag & drop
- File preview
- Removable storage
- Network locations
- Context actions
- Trash
- Keyboard navigation

Files serves as the primary dogfooding project for SolKit under complex
desktop-application scenarios.

---

# 26. Settings

Settings provides a unified system-settings experience.

Covers:

- Appearance
- Displays
- Sound
- Network
- Bluetooth
- Keyboard
- Mouse
- Touchpad
- Power
- Users
- Applications
- Privacy
- Accessibility
- Updates
- About

Settings must not bury a lot of system-implementation logic inside the UI.

Preferred layering:

```text
Settings UI
     ↓
Settings API
     ↓
System services
```

---

# 27. Terminal

SOL ships a first-party Terminal.

MVP targets:

- GPU-accelerated rendering
- Tabs
- Unicode
- True color
- Clipboard
- Search
- Configurable shell
- SolKit integration

The default shell remains whatever Linux shell the user has already chosen.

SOL does not need to re-implement a shell language for this desktop project.

---

# 28. Search & launcher

SOL provides both a Mac-like bottom Dock/Application Launcher and a unified
system-search entry point. The Dock contains pinned/running apps and a stable
Launcher entry. Launcher opens an authenticated `.app` library/grid through the
Dock or `Super+A`; search remains the faster provider-based path.

Default shortcut:

```text
Super + Space
```

Targeted capabilities:

```text
Applications
Files
Documents
Settings
Commands
Calculator
Clipboard
System actions
```

Long-term extensible to:

```text
Semantic search
Natural language
Automation
AI
```

---

# 29. System intelligence

AI is not a MVP blocker.

SOL's architecture, however, must leave the door open for future system
intelligence.

AI must not default to having arbitrary shell-execution permissions.

Preferred model:

```text
Natural language
      ↓
Intent
      ↓
Typed action
      ↓
Permission layer
      ↓
System service
```

Examples:

```text
LaunchApp
SearchFile
MoveWindow
SetVolume
OpenDocument
ChangeSetting
```

This Action API serves:

- Search
- Automation
- Accessibility
- Voice
- AI

---

# 30. Application distribution

## System components

```text
signed SOL system images
sol-pkg / sol-packaged
A/B activation through sol-boot
```

## Desktop applications

```text
signed .app bundles
content-addressed store
atomic activation and rollback
```

SOL applications distribute natively as `.app`. Repository metadata and every
bundle are signed; publisher identity, App ID, executable hashes, requested
capabilities, runtime major/minimum contract/required features, SBOM, and
provenance are covered by verification.

The Software app and `sol-pkg` use the same `sol-packaged` transaction API.
Neither a store nor an application can bypass verification or mutate the
machine-wide store directly. Interrupted installs do not affect the active
version, and retained compatible versions can be reactivated without rolling
back user data. Garbage collection protects a compatible version for every
retained known-good system deployment when one has previously been installed.

pacman/AUR may remain build inputs and developer-bootstrap tools during the
transition. Flatpak may be a compatibility subsystem. None is the installed
OS's native package, identity, permission, or trust authority.

---

# 31. Security model

SOL owns the application security policy and implements it using Linux's
existing enforcement infrastructure.

Includes:

```text
polkit
xdg-desktop-portal
Secret Service
namespaces / cgroups / seccomp
Landlock and/or a selected LSM
Wayland protocol mediation
```

Every third-party `.app` is sandboxed by default. Its signature-authenticated
App ID determines isolated data, cache, secrets, process attribution, and
grants. A signed manifest declares requestable capabilities; declaration,
installation, first-party signing, account presence, SDK choice, and previous
app versions authorize nothing by themselves.

SolKit exposes a higher-level, typed permission model. User-mediated resources
flow through portals and brokers. Every protected use requires an explicit
grant at the minimum resource/operation/duration scope. Grant persistence,
audit, and capability-handle issuance commit atomically or none becomes usable.
Unrelated capabilities cannot be accepted through one “Allow all” decision.
There is no general privileged shell escape hatch.

Durable permission identity is App ID plus verified publisher lineage; exact
bundle hash and process generation bind live handles. Same-lineage updates and
rollbacks may retain durable grants but revoke all old handles and revalidate
declaration/scope when new handles are requested. New capabilities remain
ungranted. Publisher discontinuity inherits nothing. Uninstall revokes live
leases and durable grants, so reinstall always requires new consent even if app
data was retained.

Sensitive capabilities require explicit authorization.

Examples:

- Screen recording
- Camera
- Microphone
- Location
- Input capture
- Secrets
- System settings
- Account identity and provider scopes

SOL stores device users, connected accounts, and credentials through
`sol-accountsd` and `sol-vaultd`. Applications receive opaque account handles
and short-lived scoped leases only after an explicit app × account × scope
grant. Durable refresh tokens, passkeys, private keys, and vault keys remain in
encrypted service-owned storage. Account removal revokes leases and app
associations before credentials are deleted.

`sol-securityd` is the sole permission-transaction coordinator and durable
ledger. `sol-accountsd` and `sol-vaultd` prepare unusable records under a
transaction ID and activate them only after validating the coordinator's commit
proof. Revocation advances a monotonic authorization generation before cleanup;
stale generations fail even after a participant crash.

---

# 32. Hardware compatibility

SOL MVP initially targets modern x86-64 PCs.

GPU priority:

```text
1. AMD
2. Intel
3. NVIDIA
```

The architecture must not block ARM64 support in the future.

Long-term targets:

```text
x86-64
ARM64
```

---

# 33. Hardware test matrix

The following must be tested as early as practical:

- AMD GPU
- Intel integrated GPU
- NVIDIA GPU
- Laptop
- Desktop
- Single display
- Multi-display
- HiDPI
- Mixed DPI
- Touchpad
- External mouse
- Suspend / resume
- Hotplug

The following must **never** be deferred to the end of the project:

```text
Multi-monitor
Fractional scaling
Suspend / resume
NVIDIA
Touchpad
Display hotplug
```

---

# 34. Performance targets

SOL's performance targets are not just about benchmark scores; they are about
reducing user-perceivable latency.

Focus areas:

```text
Input → frame latency
Window-resize latency
Gesture latency
Animation frame pacing
Application startup
Shell startup
Memory usage
Suspend / resume
```

Where hardware supports it, system animations must match the display's
refresh rate stably.

---

# 35. Accessibility

Accessibility must enter the architecture from SolKit's earliest days.

Includes:

- Keyboard navigation
- Screen-reader semantics
- Focus management
- Reduced motion
- High contrast
- Text scaling
- Input alternatives

Accessibility is not a post-delivery patch.

---

# 36. MVP

The earlier v0.1 desktop work proved the compositor core and established early
SDK, Shell, service, and first-party application foundations. Phases 2–6 remain
in progress as tracked by the Roadmap. The OS rebaseline defines the next MVP
as a complete developer-image lifecycle spanning the remaining desktop work and
the Phase 7–9 OS release gates.

The goal is to prove:

> SOL can boot, update, recover, install an isolated `.app`, enforce and revoke
> its permissions, and run it against a versioned SOL Runtime.

MVP includes:

### Platform

```text
Signed x86-64 UEFI boot path
Two bootable system slots
Slot-bound kernel/initrd/root-image deployment manifests
Redundant trial-updated sol-boot and recovery copies
Known-good deployment and firmware-visible boot fallback
Reproducible SOL system deployment
```

### Desktop

```text
Window management
Workspace
Basic multi-monitor
Launcher
Dock
Top bar
Notifications
Wallpaper
```

### SDK

```text
SolKit core
SolUI
SolApp
Basic commands
Basic system API
Design tokens
Fluid material roles with solid accessibility fallbacks
sol-runtime-1 major + minimum contract revision + feature descriptor
```

### Applications

```text
Files
Terminal
Settings
One signed third-party sample .app
```

### Package and security

```text
sol-pkg / sol-packaged transaction path
Signed, inspectable, self-contained .app
Content-addressed install and atomic rollback
Per-system-version compatible app activation
Default-deny sandbox
Explicit minimum-scope atomic grant + audit + lease
Defined update/uninstall/reinstall grant inheritance
System-managed account and encrypted credential fixture
Coordinator-backed portal/account grant, revocation, and audit
```

---

# 37. Non-goals for MVP

The OS MVP does **not** require:

- Self-built Linux kernel
- Self-built init system
- Self-built audio stack
- Self-built network stack
- A commercial app catalog or payment system
- Full AI assistant
- Mobile support
- Cloud ecosystem
- Office suite
- A custom driver stack or cryptographic primitive
- Long-term support for more than the first SOL Runtime major
- Mobile and non-x86-64 boot targets

These omissions do not relax verified boot, transactional rollback, `.app`
self-containment, or default-deny permission requirements.

---

# 38. Development phases

## Phase 0 — Foundation

Goal:

```text
Arch development environment
Rust workspace
Smithay compositor
Basic Wayland client
Input
Basic rendering
```

Success criterion:

> Start a standalone SOL Wayland session and run standard Wayland applications.

---

## Phase 1 — Desktop Core

Required deliverables (Phase 1 reopened for integration and real-session
closure; implementation status is owned by the Roadmap):

```text
Window management
Focus
Move / resize
Workspace
Multi-monitor
Basic shell IPC
```

Success criterion:

> SOL can be used as a basic daily-use Wayland compositor.

---

## Phase 2 — SolKit

In-progress scope:

```text
SolUI
SolApp
Window
View
Layout
Input
Commands
Design tokens
Animation
```

Success criterion:

> An app with SOL-native look and interaction can be fully developed using
> SolKit alone.

---

## Phase 3 — First-party applications

In-progress scope:

```text
Settings
Terminal
Files
```

SolKit is iterated in parallel, driven by actual app requirements.

Success criterion:

> The three first-party apps share a unified UX — they do not each
> re-implement system behavior separately.

---

## Phase 4 — Shell experience

In-progress scope:

```text
Dock
Launcher
Overview
Notifications
Quick settings
Touchpad gestures
Animations
```

Success criterion:

> SOL forms a complete, coherent desktop interaction model.

---

## Phase 5 — Daily driver

In-progress scope:

```text
Suspend / resume
Multi-monitor
Fractional scaling
NVIDIA
Clipboard
Drag & drop
Screen sharing
Screen recording
Bluetooth
Audio
Power
Application compatibility
Input method (IME)
```

Success criterion:

> A developer can use SOL as their primary desktop environment long-term.

---

## Phase 6 — Developer platform

In-progress scope:

```text
Public SolKit
Documentation
Templates
Developer tools
Packaging
Transitional Arch build packaging
SDK stability
```

Success criterion:

> A third-party developer can build a high-quality native app without
> understanding SOL internals.

The original phases above remain the history of the desktop substrate. The OS
rebaseline continues with the following release gates.

## Phase 7 — OS foundation

Planned deliverables:

```text
Reproducible system-image composition
Signed redundant UEFI sol-boot path with one-shot trial activation
A/B deployment manifests binding kernel, initrd, root image, and runtime state
Slot selection and boot-success protocol
Redundant recovery environment with independent fallback
Installer for supported x86-64 hardware
```

Success criterion:

> A failed or corrupted staged system update automatically returns to a known-
> good bootable SOL image, and recovery works without the graphical shell.

## Phase 8 — Native application platform

Planned deliverables:

```text
.app bundle builder, verifier, and inspector
sol-pkg client and sol-packaged transaction service
Signed repositories and content-addressed application store
Atomic install, update, removal, and rollback
sol-securityd default-deny sandbox and permission grants
Atomic permission ledger: grant + audit + lease/consumption
Same-lineage update and uninstall/reinstall grant semantics
sol-securityd coordinator + prepared sol-accountsd / sol-vaultd participants
Portal/broker integration, revocation, and audit
```

Success criterion:

> Two apps carrying incompatible dependencies run independently; an
> interrupted update leaves the active app intact; undeclared or revoked
> capabilities fail at the enforcement boundary.

## Phase 9 — Runtime and ecosystem

Planned deliverables:

```text
sol-runtime-1 major/revision/feature ABI/IPC descriptor
SolKit bindings, templates, and .app packaging pipeline
Compatibility packaging for major Wayland-native Linux stacks
Software catalog client over sol-packaged
Runtime side-by-side, compatibility resolution, fallback, and retention policy
Compositor-backed SOL fluid material rendering and fallbacks
Bundled sol-gtk / sol-qt adapters over stable ABI/IPC
```

Success criterion:

> An external developer builds and signs a small `.app` that vendors only its
> non-SOL dependencies, installs through `sol-pkg`, and uses protected system
> capabilities and accounts solely through explicitly granted SOL framework
> APIs, while system materials preserve accessibility and frame budgets; OS
> rollback resolves a compatible retained app version or an explicit unavailable
> state without blocking boot or rewinding app data.

---

# 39. Repository strategy

An early monorepo is acceptable:

```text
sol/
├── compositor/
├── shell/
├── sdk/
│   ├── ui/
│   ├── app/
│   ├── graphics/
│   ├── animation/
│   └── system/
├── services/
├── boot/
│   ├── sol-boot/
│   ├── image/
│   └── recovery/
├── security/
├── accounts/
├── compat/
├── apps/
│   ├── files/
│   ├── terminal/
│   └── settings/
├── protocols/
├── packaging/
│   ├── sol/
│   └── arch/        # transitional bootstrap only
├── tests/
└── docs/
```

An early monorepo enables the compositor, SDK, shell, and first-party apps to
evolve in lockstep quickly.

The repository boundary is re-evaluated after API stabilization.

---

# 40. Core technology decisions

Currently settled:

| Item | Decision |
|---|---|
| Product boundary | Complete Linux-kernel operating system |
| Build inputs | Upstream projects; Arch permitted for bootstrap during transition |
| Primary language | Rust |
| Display protocol | Wayland |
| Compositor framework | Smithay |
| X11 compatibility | Not provided (focus on Wayland-native) |
| Audio | PipeWire |
| Networking | NetworkManager |
| Bluetooth | BlueZ |
| Init / services | systemd |
| Boot | Redundant signed UEFI/recovery copies with trial activation; slot-bound A/B deployments |
| System package manager | `sol-pkg` + `sol-packaged`; boot/recovery/system/app transactions |
| Native app format | Signed, self-contained `.app` bundle |
| App distribution | Signed content-addressed installs; per-deployment compatible-version resolution |
| App isolation | Default-deny sandbox + typed portals/brokers |
| Permission grants | App ID/publisher-lineage durable identity; coordinator-atomic grant/audit/lease; defined update/uninstall inheritance |
| Accounts | `sol-securityd` coordination + prepared `sol-accountsd`/encrypted `sol-vaultd` participants; apps receive generation-fenced handles |
| System material | Semantic SOL fluid material with solid accessibility fallbacks |
| Shared runtime | Side-by-side major + monotonic contract revision + named-feature ABI/IPC descriptors |
| Non-native toolkits | Private bundled runtime + optional bundled SOL adapter |
| Compatibility levels | Native / Integrated / Compatible; equal security and system capabilities |
| First-party SDK | SolKit |
| First-party UI | SolUI |
| Desktop shell | SOL Shell |
| Shell layout | Bottom Dock; foreground menu upper-left; trusted status/Live Capsule upper-right |
| Window controls | Physical upper-left: Close, Minimize, Maximize/Restore |
| Live activities | Shell-rendered declarative Live Capsule; broker-authoritative privacy state |
| Desktop compositor | SOL Compositor |
| Input method | First-party frontend (sol-ime) + fcitx5 engine |

---

# 41. Decision register

The numbering in this register is stable because ADRs and other documents cite
it. Items marked **Accepted** are closed at the stated contract level; any
remaining implementation choice is described explicitly. Unmarked items remain
open and must be decided during prototyping.

1. **Accepted — ADR-0004:** Slint-backed SolUI rendering architecture
2. **Accepted — ADR-0004:** retained/reactive declarative UI model
3. Whether the Smithay renderer is sufficient long-term
4. Long-term role of Vulkan / wgpu
5. Compositor ↔ Shell IPC protocol (transport)
6. **Accepted — ADR-0011:** daemon-owned typed settings storage boundary
7. **Accepted — ADR-0012:** validated reverse-DNS `AppId`
8. **Accepted — ADR-0017:** source-API stability tiers; no Rust ABI promise
9. Server-side vs. client-side decoration policy
10. Global menu implementation protocol (existence/upper-left placement settled
    by ADR-0025)
11. Window-tiling product model (specifics)
12. Exact LSM and sandbox composition after kernel-level prototypes (the
    default-deny policy and portal boundary are settled)
13. Upstream intake cadence and SOL release-channel policy
14. Installer implementation and disk-layout details
15. Software catalog metadata and optional commerce model (`sol-packaged`
    remains the settled install authority)
16. **Accepted — ADR-0014:** bounded local application-catalog search index
17. **Accepted — ADR-0013:** typed, caller-attributed System Action API
18. **Accepted — ADR-0016:** privacy-bounded diagnostics foundation
19. IME engine/frontend integration boundary — fcitx5 addon language
    coverage; sol-ime frontend owns candidate-window UI; engine-upgrade
    strategy; when a custom engine is ever considered
20. `.app` deterministic container encoding and compression
21. SOL Runtime ABI generator, feature-registry encoding, and IPC schema
    technology (major/revision/feature compatibility model settled by ADR-0020)
22. Boot measurement, key enrollment, hardware-backed attestation, and exact EFI
    entry encoding (redundancy/trial/fallback settled by ADR-0019)
23. System-image filesystem and delta-update encoding
24. Account vault database and hardware-sealing implementation
25. Fluid-material compositor sampling/refraction path and fallback thresholds
26. Toolkit-adapter implementation matrix and semantic-material Wayland schema
27. Live Activity registration/menu/status-item IPC schema and persistence

---

# 42. Long-term product direction

SOL's long-term goal is not to become:

> Another desktop environment installed on somebody else's operating system.

But rather to establish:

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
SOL Applications
  ↓
Third-party Applications
```

SOL's long-term technical assets focus on:

**Boot + System Image + Package Manager + Security + Compositor + Framework
Runtime + Design System + Applications**

SolKit is the core bridge between the system experience and the application
ecosystem.

Ultimately, users should not perceive internal system complexity because an
application happens to run on Linux, Wayland, or any other underlying
technology.

What users see should simply be:

> SOL.
