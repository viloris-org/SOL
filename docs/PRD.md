# SOL Desktop Platform — Product Requirements Document

**Version:** v0.1
**Status:** Concept / Pre-Alpha
**Platform:** Linux / Arch Linux
**Project name:** SOL
**Core technology stack:** Rust + Wayland + Smithay

---

# 1. Product overview

SOL is a modern Linux desktop platform built on Arch Linux.

SOL's goal is not to produce a Linux theme that mimics the look of macOS, nor is
it a simple fork of GNOME, KDE Plasma, or any other existing desktop environment.
Instead, SOL re-designs the Linux graphical desktop experience from the platform
layer upward.

SOL will own its own:

- Wayland compositor
- Desktop shell
- Application SDK
- Design system
- System services
- First-party applications
- Application-distribution experience

The foundation continues to leverage mature Linux infrastructure, including the
Linux kernel, systemd, PipeWire, NetworkManager, BlueZ, Mesa, polkit, udisks2,
and others.

SOL's core principle is:

> Do not reinvent Linux. Redesign the Linux desktop.

---

# 2. Product vision

SOL aims to deliver a Linux desktop platform that meets the quality bar of
modern commercial desktop operating systems.

Key goals include:

- Highly consistent system UX
- High-performance, low-latency graphics experience
- Complete touchpad and gesture interaction
- Excellent HiDPI and multi-monitor support
- Cohesive first-party application experience
- Full application-development SDK
- Compatibility with the broader Linux application ecosystem
- Remaining open to power users while preserving core Linux flexibility
- Hiding unnecessary Linux system complexity from casual users

SOL does not treat "highly customizable" as a primary goal.

Rather than letting each component have a radically different look and
behavior, SOL emphasizes **coherent, predictable, and polished** behavior.

---

# 3. Product positioning

SOL should be understood as:

> A modern Linux desktop platform built on Arch Linux.

SOL is **not** equivalent to a Linux distribution.

The relationship is:

```text
Arch Linux
      │
      ▼
SOL Platform
├── Compositor
├── Desktop Runtime
├── System Services
├── Application Framework
├── Design System
└── First-party Applications
```

Future releases may offer:

```text
SOL Desktop
```

Installable onto compatible Arch Linux systems.

And:

```text
SOL OS
```

An Arch-based Linux distribution that ships SOL Desktop pre-configured.

The two should remain architecturally decoupled.

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
- Flatpak

First-party apps prioritize SolKit.

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
│                Linux Platform                │
│                                              │
│ Arch / systemd / Mesa / PipeWire / etc.     │
└──────────────────────────────────────────────┘
```

---

# 6. Base system

SOL initially attaches to Arch Linux.

SOL does not fork Arch Linux's core infrastructure.

Priority reuses:

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
Flatpak
pacman
```

SOL maintains its own desktop-platform components.

---

# 7. Package architecture

SOL ships official packages through an Arch repository:

```text
[sol-core]
[sol-apps]
[sol-sdk]
```

## sol-core

Includes:

```text
sol-compositor
sol-shell
sol-session
sol-settingsd
sol-notificationd
sol-portal
sol-polkit-agent
sol-ime
sol-desktop
```

## sol-apps

Includes:

```text
sol-files
sol-terminal
sol-settings
sol-store
sol-viewer
sol-monitor
```

## sol-sdk

Includes:

```text
solkit
sol-ui
sol-sdk
sol-sdk-docs
```

Also providing:

```text
sol-desktop
```

as the meta package.

Target installation:

```bash
sudo pacman -S sol-desktop
```

installs a complete SOL Desktop on a compatible Arch Linux system.

---

# 8. Update model

Arch Linux is SOL's upstream package source.

SOL must not follow Arch's bleeding-edge roll indefinitely.

The recommended update path:

```text
Arch upstream
      ↓
SOL Integration
      ↓
SOL Testing
      ↓
SOL Stable
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

Long-term, an atomic/image-based system may be evaluated; it is not a
requirement for MVP.

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

SolKit is SOL's application framework.

SolKit is not merely a widget toolkit.

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
`sol-design`, and cannot reach the same visual consistency.

SOL treats the two consistency goals separately:

```text
First-party + SolKit apps     → architecturally enforced consistency (strong guarantee)
Third-party Linux apps        → rely on co-existence conventions in the desktop (best-effort)
```

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
```

Goal: third-party developers can produce apps nearly as polished as
first-party apps — there should not be an artificial first-party API
advantage.

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

SOL provides a unified system-search entry point.

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
pacman
```

## Desktop applications

```text
pacman
AUR
```

SOL desktop applications distribute primarily through pacman / AUR.

SOL maintains its own Arch / AUR repositories:

```text
[sol-core]
[sol-apps]
[sol-sdk]
```

If SOL ships a store later, it should hide package-implementation details
that ordinary users do not need to understand, while using pacman / AUR as
the real delivery mechanism underneath.

Power users may still use directly:

```text
pacman
AUR helper / manual PKGBUILD
makepkg
```

AUR is **not** part of SOL's official application trust chain. Official apps
ship from signed `[sol-*]` repositories; AUR packages are community-maintained.

---

# 31. Security model

SOL leverages Linux's existing security infrastructure wherever possible.

Includes:

```text
polkit
xdg-desktop-portal
Secret Service
Linux permissions
systemd
```

Application sandboxing (Flatpak sandbox, or another sandbox mechanism SOL
evaluates separately) is not a mandatory MVP item; its decision is deferred
(PRD §41 item #12).

SolKit's system API should expose a higher-level permission model.

Sensitive capabilities require explicit authorization.

Examples:

- Screen recording
- Camera
- Microphone
- Location
- Input capture
- Secrets
- System settings

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

SOL v0.1 MVP does not aim to build a complete OS.

The goal is to prove:

> SOL's compositor, SDK, shell, and first-party applications can form a
> coherent desktop platform.

MVP includes:

### Platform

```text
Arch Linux base
SOL session
Wayland
Smithay compositor
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
```

### Applications

```text
Files
Terminal
Settings
```

---

# 37. Non-goals for MVP

v0.1 does **not** require:

- Self-built Linux kernel
- Self-built init system
- Self-built audio stack
- Self-built network stack
- Full app store
- Full AI assistant
- Mobile support
- Cloud ecosystem
- Office suite
- Full immutable OS
- Full third-party SDK stability promises

None of these capabilities must block core-desktop validation.

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

Implemented:

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

Implemented:

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

Implemented:

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

Enhanced:

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

Focus on resolving:

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

Perfected:

```text
Public SolKit
Documentation
Templates
Developer tools
Packaging
pacman / AUR packaging integration
SDK stability
```

Success criterion:

> A third-party developer can build a high-quality native app without
> understanding SOL internals.

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
├── apps/
│   ├── files/
│   ├── terminal/
│   └── settings/
├── protocols/
├── packaging/
│   └── arch/
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
| Base distribution | Arch Linux |
| Primary language | Rust |
| Display protocol | Wayland |
| Compositor framework | Smithay |
| X11 compatibility | Not provided (focus on Wayland-native) |
| Audio | PipeWire |
| Networking | NetworkManager |
| Bluetooth | BlueZ |
| Init / services | systemd |
| System package manager | pacman |
| App distribution | pacman / AUR |
| First-party SDK | SolKit |
| First-party UI | SolUI |
| Desktop shell | SOL Shell |
| Desktop compositor | SOL Compositor |
| Input method | First-party frontend (sol-ime) + fcitx5 engine |

---

# 41. Open decisions

The following items need to be decided during prototyping:

1. SolUI rendering architecture
2. Retained vs. reactive/declarative UI model
3. Whether the Smithay renderer is sufficient long-term
4. Long-term role of Vulkan / wgpu
5. Compositor ↔ Shell IPC protocol (transport)
6. Settings storage architecture
7. Application identity format
8. SolKit ABI/API stability strategy
9. Server-side vs. client-side decoration policy
10. Whether a global menu exists
11. Window-tiling product model (specifics)
12. Application sandbox default policy (evaluate SOL sandbox or reuse
    portals; not required for MVP — Flatpak-first abandoned in favor of
    pacman/AUR per §30)
13. SOL Stable vs. Arch rolling-release sync strategy
14. Installer technology route
15. Store backend
16. Search-index architecture
17. System Action API
18. Crash reporting / diagnostics architecture
19. IME engine/frontend integration boundary — fcitx5 addon language
    coverage; sol-ime frontend owns candidate-window UI; engine-upgrade
    strategy; when a custom engine is ever considered

---

# 42. Long-term product direction

SOL's long-term goal is not to become:

> Another Linux desktop environment.

But rather to establish:

```text
Linux
  ↓
SOL Desktop Runtime
  ↓
SolKit
  ↓
SOL Applications
  ↓
Third-party Applications
```

SOL's long-term technical assets focus on:

**Compositor + Desktop Runtime + Application Framework + Design System +
First-party Applications**

SolKit is the core bridge between the system experience and the application
ecosystem.

Ultimately, users should not perceive internal system complexity because an
application happens to run on Linux, Wayland, pacman/AUR, or any other
underlying technology.

What users see should simply be:

> SOL.
