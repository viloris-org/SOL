# SOL Shell Spatial and Live Activity Contract

**Status:** Normative product direction; compositor/surface implementation is incomplete
**Baseline:** 2026-08-22

This document fixes the spatial grammar of SOL Shell. It defines where global
controls live, how applications integrate, and which surfaces are trusted.

## 1. Desktop spatial grammar

```text
┌──────────────────────────────────────────────────────────────────────────┐
│ Active App  File  Edit  View  …       Live Capsule  Tray  Info  Status │
├──────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌ window controls ┐ ┌ Sidebar ┐            Application Content         │
│  │ ●  ●  ●         │ │         │                                        │
│  └─────────────────┘ │         │                                        │
│                      └─────────┘                                        │
│                                                                          │
│                       ┌──────────────┐                                   │
│                       │  SOL Dock    │                                   │
│                       └──────────────┘                                   │
└──────────────────────────────────────────────────────────────────────────┘
```

The placement is physical and stable:

- the foreground application's identity and global menu occupy the upper left;
- system information, Live Capsule, application status items, Notification
  Center, and system status occupy the upper right;
- application window controls occupy the upper-left corner of the window
  decoration;
- the Dock is centered at the bottom edge;
- application navigation sidebars use the SOL Sidebar material on the left by
  default, while app content remains solid.

Localized menu contents may use right-to-left text/layout internally, but the
global application and system zones do not swap sides.

## 2. Dock

The SOL Dock is a first-party Shell surface using `Material::Dock`. It is not a
generic taskbar skin.

It provides:

- a stable Application Launcher entry;
- user-pinned applications;
- running and focused state;
- launch, activate, minimize/restore, window list, and quit actions;
- notification badges supplied through authenticated system APIs;
- drag reordering and pin/unpin with recoverable state;
- optional intelligent auto-hide without changing its bottom anchor.

Press feedback begins immediately. Launch/activate motion originates at the
icon and remains interruptible. The Dock does not use looping bounce; momentum
or overshoot is reserved for a user-driven drag. The hit target does not shrink
with the visible icon.

On multiple displays, one full Dock belongs to the active output. Edge intent
can reveal it on another output without moving windows or changing focus.

## 3. Application Launcher and search

Application Launcher is a Shell-owned library/grid backed by the authenticated
`.app` catalog. It opens from the Dock and `Super+A`. `Super+Space` remains the
unified search entry for apps, files, settings, commands, and other providers.

Launcher results use App ID, active bundle hash, publisher, and current install
state. Launch requests pass through the typed action boundary. Applications
cannot purchase visual priority, inject unverified entries, or impersonate a
different App ID.

The Launcher materializes from its Dock anchor. Keyboard navigation,
accessibility, text scaling, reduced motion, and reduced transparency are part
of the same contract as pointer/touch use.

## 4. Window controls

SOL-native window controls are physically placed at the upper left of the
window decoration in this fixed order:

```text
Close  Minimize  Maximize/Restore
```

They have stable accessibility names and enlarged invisible hit targets. Press
feedback occurs on pointer-down; destructive close still follows the
application's document/lifecycle safeguards.

SolKit uses this layout automatically. Official GTK/Qt adapters request the
same layout through supported toolkit/window-decoration APIs. A generic
client-side-decorated Wayland application may retain its own buttons; SOL does
not patch private toolkit internals to force the layout. Server-side
decorations, when used, follow the SOL order.

## 5. Application Sidebar material

Navigation sidebars use `Material::Sidebar`, a thicker and more separating
fluid material than small controls. The sidebar may float over scrolling
content or sit beside solid content, but dense document/list content remains
solid.

Rules:

- sidebar text/icons use system-resolved contrast and vibrancy;
- content uses a soft scroll-edge transition instead of a permanent hard line;
- a glass sidebar cannot contain another full glass panel; nested surfaces
  consolidate or become solid;
- reduced transparency and high contrast resolve the sidebar to an opaque
  bounded surface without changing layout;
- GTK/Qt can request the semantic role through the official adapter/material
  protocol, but never receives backdrop pixels.

## 6. Foreground application menu

The upper-left global menu always represents the compositor-authenticated
foreground application. Its owner is derived from focused surface → process →
App ID, not from a client-provided display name alone.

SolKit exports menus from its command graph. GTK/Qt adapters translate supported
`GMenu`/action and `QMenu`/action models into the same versioned menu protocol.
Electron and other adapters may do the same. Menu updates are atomic snapshots,
so focus changes cannot leave commands from two applications mixed together.

If an app exports no menu, the Shell shows its verified application identity
and only safe standard lifecycle/window commands. Overflow collapses into a
More menu before it can collide with the upper-right system zone.

## 7. Upper-right information and status zone

The upper-right zone is Shell-owned and ordered by function:

```text
Live Capsule → application status items → Notification Center → system status
```

System status includes time, connectivity, audio, power, accessibility state,
and Quick Settings entry. Notification Center contains durable informational
events. Ongoing activities belong in Live Capsule rather than repeatedly
posting notifications.

The application tray is a typed status-item registry, not arbitrary embedded
client windows. A status item supplies authenticated App ID, semantic icon,
short label, state, and typed actions. Shell controls size, material, input,
overflow, accessibility, and rate limits. Legacy tray protocols may be bridged
through a constrained compatibility adapter but receive no extra authority.

## 8. Live Capsule

Live Capsule is one trusted, expandable Shell control using
`Material::Capsule`. Applications register live activities; they do not render
inside Shell chrome.

Appropriate activities include:

- audio/video recording and calls;
- timers and alarms;
- screen sharing or remote-control sessions;
- bounded transfers, exports, builds, or media playback;
- system privacy indications for microphone, camera, screen capture, location,
  or remote control.

An application registration contains only declarative data:

```text
LiveActivity
├── authenticated AppId + activity id
├── activity kind + lifecycle lease
├── short title / compact value / elapsed time / bounded progress
├── semantic icon and urgency
└── typed actions: Open, Pause, Resume, Stop, End, Dismiss
```

Arbitrary markup, executable callbacks, raw shell commands, remote images, ads,
and unbounded animation are forbidden. Actions return to the attributed app or
system broker through typed IPC. Stop/End for a broker-owned recording or
capture revokes/ends the real underlying session, not merely the indicator.

### Registration and authority

- Apps declare `shell.live-activity` and receive an explicit atomic grant before
  creating application-owned capsules.
- Each activity is bound to App ID, bundle lineage, user session, process/
  service owner, and an expiring lifecycle lease.
- App crash, logout, any bundle activation/replacement, lease expiry, or service
  completion removes the registration deterministically. A same-lineage update
  may retain the durable `shell.live-activity` grant, but the new process must
  create a fresh release-bound registration and lease.
- Registration grants presentation only. It does not grant microphone, camera,
  capture, account, network, or background-execution authority.

### Privacy activities

Microphone, camera, screen capture, location, and remote-control indicators are
created by the trusted capability broker from actual active leases. They do not
depend on an application's capsule registration and cannot be hidden,
downgraded, recolored, or replaced by the app.

The collapsed capsule names the responsible application and capability. The
expanded view shows start time/duration, active device or shared target where
safe, and system-owned Stop/Revoke controls. A critical privacy activity takes
priority over cosmetic application activities.

### Multiple activities

There is one physical capsule anchor per top bar. The Shell multiplexes multiple
registrations into a compact stack/count and an expanded activity list.
Privacy/security activities sort first, then calls/recording, time-sensitive
activities, and progress/status work. Applications cannot purchase or request
priority outside their validated activity kind.

### Motion and accessibility

Expansion grows from the capsule's upper-right anchor and collapses along the
same path. It starts from the current presentation value, is interruptible, and
never blocks Stop/End input during motion. Reduced motion uses a short cross-
fade/static size change; reduced transparency/high contrast uses the solid
Capsule material. Every compact state and action has an accessibility label and
keyboard path.

## 9. Multi-display behavior

Every output has a top bar. The foreground app menu appears on the output
containing the focused window. System status remains available on every output.
Privacy indicators are mirrored on every output; the full activity capsule
lives on the active output and other outputs show a compact privacy/activity
presence indicator. Expansion occurs on the output where the user invoked it.

## 10. Trust boundaries and acceptance

The Shell is the renderer and input owner for global menus, status items, and
Live Capsule. Client applications provide authenticated declarative state only.

Required tests include:

1. focus changes atomically replace the upper-left menu without mixed commands;
2. a client cannot claim another app's menu, tray item, badge, or capsule;
3. stopping a system recording capsule ends the actual broker session;
4. microphone/camera indicators survive app UI failure and cannot be hidden;
5. stale leases disappear after crash/logout/replacement and cannot replay;
6. multiple capsules prioritize privacy and remain keyboard/screen-reader usable;
7. Dock/Launcher/global menu/right zone remain usable at minimum width, text
   scaling, fractional scale, and multi-display configurations;
8. Sidebar/Dock/Capsule materials fall back to solid without layout change;
9. GTK/Qt menu/status/material adapters retain the same attribution and
   permission behavior as SolKit clients.
