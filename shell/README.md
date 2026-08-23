# sol-shell

SOL's desktop shell: top bar, dock, launcher, overview, notification center,
quick settings, global application menu, typed application status, Live
Capsule, and system overlays. (PRD §11; ADR-0003 explicitly rules out
Quickshell; ADR-0025 fixes the spatial/trust contract.)

See [Shell Spatial and Live Activity Contract](../docs/shell-experience.md) for
the normative layout and behavior.

## Position in the architecture

```
sol-compositor
       ↕  Typed IPC (transport undecided — decision #5 / ADR-0006)
sol-shell
```

- The Shell owns desktop UI and system overlays; the compositor owns
  surfaces / windows / input / focus / workspaces / outputs.
- **A shell crash must not take the compositor down with it** (PRD §11 hard
  constraint).
- The Shell's UI ultimately stands on SolKit (`sol-ui` + `sol-design` +
  `sol-animation`); there is deliberately no second, parallel UI stack.

## Status

**Phase 4 foundations in progress.** The repository contains renderer-neutral
Dock/Launcher, top-bar, overview, notifications, Quick Settings, consent, and
overlay models. Native compositor-backed surfaces remain incomplete.

- The first shell surface (layer-shell top bar) is a **Phase 1** Milestone M1
  deliverable.
- The full desktop interaction model (Dock / Launcher / Overview / notifications /
  quick settings / global menu / Live Capsule / touchpad gestures) is **Phase 4**.

## Fixed shell geography

- Bottom center: SOL Dock and Application Launcher.
- Window upper-left: Close, Minimize, Maximize/Restore.
- Screen upper-left: compositor-authenticated foreground app menu.
- Screen upper-right: Live Capsule, typed app status items, Notification Center,
  and system status/Quick Settings.
- Application navigation: `Material::Sidebar`; dense content stays solid.

Live Capsule is Shell-rendered from attributed declarative registrations.
Microphone, camera, capture, location, and remote-control indicators come from
the capability broker and cannot be hidden by an app.

## Key dependencies

- Compositor `layer-shell` protocol integration (Phase 1)
- Compositor↔shell typed IPC transport decision (decision #5)
- SolKit maturity (Phase 2) — Phase 3/4 shell UI is built on it

## Positioning

SOL Shell is a first-party SOL asset, not a thin wrapper around a generic
layer-shell shell (Hyprland/niri style). It is deeply bound to our Smithay
compositor's typed IPC and SolKit/SolUI's token/animation system. (ADR-0003)
