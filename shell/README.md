# sol-shell

SOL's desktop shell: top bar, dock, launcher, overview, notification center,
quick settings, and system overlays. (PRD §11; ADR-0003 explicitly rules out
Quickshell.)

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

**Phase 0 scaffold** (`main.rs` only prints a notice; nothing implemented).

- The first shell surface (layer-shell top bar) is a **Phase 1** Milestone M1
  deliverable.
- The full desktop interaction model (Dock / Launcher / Overview / notifications /
  quick settings / touchpad gestures) is **Phase 4**.

## Key dependencies

- Compositor `layer-shell` protocol integration (Phase 1)
- Compositor↔shell typed IPC transport decision (decision #5)
- SolKit maturity (Phase 2) — Phase 3/4 shell UI is built on it

## Positioning

SOL Shell is a first-party SOL asset, not a thin wrapper around a generic
layer-shell shell (Hyprland/niri style). It is deeply bound to our Smithay
compositor's typed IPC and SolKit/SolUI's token/animation system. (ADR-0003)
