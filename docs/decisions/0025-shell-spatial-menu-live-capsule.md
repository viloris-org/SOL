# ADR-0025: Shell spatial grammar, global menu, and Live Capsule

- **Status:** Accepted (product/architecture; implementation incomplete)
- **Date:** 2026-08-22
- **Target phase:** Phase 4 and Phase 9
- **Extends:** ADR-0015, ADR-0021, ADR-0023, ADR-0024

## Context

Dock, launcher, menus, status items, privacy indicators, and live activities
must form one predictable system rather than compete for arbitrary screen
positions. Application-drawn global chrome also creates spoofing and privacy
risks, especially for microphone, camera, recording, and remote-control state.

## Decision

SOL fixes the following physical layout:

- bottom center: first-party SOL Dock with Application Launcher entry;
- window upper left: Close, Minimize, Maximize/Restore controls;
- screen upper left: verified foreground app identity and global application
  menu;
- screen upper right: Live Capsule, typed application status items,
  Notification Center, and system status/Quick Settings;
- application leading sidebar: `Material::Sidebar` by default for SolUI
  navigation, with solid dense content beside it.

Top-level zones do not swap sides for RTL locales; their internal content does.
Every output has a top bar, while focus, expansion, and Dock ownership follow the
active output rules in the Shell contract.

Global menus are Shell-rendered atomic snapshots bound to compositor focus and
authenticated App ID. SolKit exports commands directly; GTK/Qt adapters may
translate supported public menu/action models. An app without an exported menu
cannot inject arbitrary UI into the upper-left zone.

Application tray/status items are declarative typed records, not embedded
client windows. The Shell owns size, overflow, material, focus, accessibility,
and action dispatch.

Live Capsule is a single trusted upper-right anchor that multiplexes multiple
declarative live activities. Application capsules require a declared
`shell.live-activity` capability and explicit atomic grant. Registration grants
presentation only, never the underlying microphone/camera/capture/background
capability.

Privacy capsules are created by trusted brokers from real active leases. Apps
cannot hide or replace them. Their system-owned Stop/Revoke action terminates
the underlying session. App registrations are bound to App ID, publisher/bundle
lineage, user/session, owner process/service, and an expiring lease.

Dock, Sidebar, and Capsule become formal `sol-design` material roles. Their
motion originates from the visible anchor, starts from live presentation state,
is interruptible, and has reduced-motion/transparency/high-contrast fallbacks.

## Consequences

- SOL gains a stable Mac-like spatial model without copying implementation or
  allowing applications to own trusted chrome.
- A global menu protocol, typed status-item protocol, Live Activity API, and
  capability-broker activity feed become Shell platform work.
- `sol-gtk`/`sol-qt` need menu/action, status, window-control, and material-role
  adapters where public toolkit APIs allow them.
- At narrow widths, app-menu overflow collapses before the trusted right zone;
  privacy indicators are never dropped.
- Notifications describe events; Live Capsule owns ongoing state and immediate
  controls.

## Required tests

- Focus change cannot mix or spoof application menu commands.
- Tray/capsule registrations are owner-attributed, rate-limited, and removed on
  crash, bundle activation/replacement, or lease expiry; a retained durable grant
  never retains the old release/process-bound registration.
- Stop/Revoke from a privacy capsule ends the broker session.
- Privacy activity remains visible if the responsible app hangs or crashes.
- Multiple activities preserve security priority and accessible navigation.
- All fixed zones survive localization, text scaling, narrow outputs,
  fractional scaling, reduced transparency/motion, and multiple displays.

## Non-claims

This ADR does not implement the layer-shell surfaces, global-menu protocol,
Live Activity service, or toolkit adapters. The renderer-neutral Dock/Launcher
and top-bar foundations do not yet satisfy this complete contract.

## Related

- [Shell Spatial and Live Activity Contract](../shell-experience.md)
- ADR-0015 (system overlay and popup contract)
- ADR-0021 (atomic permissions)
- ADR-0023 (fluid material)
- ADR-0024 (toolkit compatibility)
