# 8. Distribution and X11 scope: pacman/AUR-first, drop XWayland

- **Status:** Partially superseded by ADR-0019 and ADR-0020 (X11 decision remains accepted)
- **Date:** 2026-08-15
- **Target phase:** Phase 1+ (Distribution throughout)

## Context

Two related scope decisions were folded into the PRD on 2026-08-15 and are
recorded here so the first-party install / compatibility story is explicit.

## Distribution

> Historical distribution decision: ADR-0019 and ADR-0020 supersede this
> section after SOL became a complete OS. The X11/XWayland section below
> remains current.

SOL desktop applications now distribute via the native `sol-pkg` system and `.app`
bundles (see ADR-0020). A SOL Store (if implemented) manages discovery and
installation through this delivery mechanism.

## X11 / XWayland

SOL provides **no X11 session and no XWayland** (PRD §4.2 "Wayland Native").

- Traditional X11 applications are not a compatibility target. SOL focuses on
  the modern ecosystem: GTK, Qt, SDL, Flutter, Electron, Wayland-native apps —
  all on Wayland.
- Dropped from compositor architecture (§10), MVP platform (§36), Phase 1
  (§38), and hardware test matrix (§33).
- Acceptance: third-party GTK/Qt/Electron apps cannot share SOL's exact
  visual consistency (PRD §19.1) — that is out of the platform's control; SOL
  guarantees consistency for first-party + SolKit apps by architecture.

## Consequences

- Flatpak remains supported only as a third-party app runtime (§4.5/§6), not a
  first-class SOL distribution channel.
- The compositor never needs to maintain an XWayland path — a simplification
  vs a mixed-protocol compositor.

## Related

- PRD §7 (Package Architecture), §30 (Distribution), §31 (Security Model),
  §4.2 (Wayland Native), §19.1 (consistency boundaries); ADR-0007 (IME);
  ADR-0001 (monorepo for packaging).
