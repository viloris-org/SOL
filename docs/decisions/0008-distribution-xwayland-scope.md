# 8. Distribution and X11 scope: pacman/AUR-first, drop XWayland

- **Status:** Accepted
- **Date:** 2026-08-15
- **Target phase:** Phase 1+ (Distribution throughout)

## Context

Two related scope decisions were folded into the PRD on 2026-08-15 and are
recorded here so the first-party install / compatibility story is explicit.

## Distribution

SOL desktop applications distribute via **pacman / AUR**, not Flatpak-first
(PRD §30, §5). SOL maintains its own Arch repositories:

```text
[sol-core]
[sol-apps]
[sol-sdk]
```

`sol-desktop` is the meta package; target install is:

```bash
sudo pacman -S sol-desktop
```

- Official applications are signed and shipped from the `[sol-*]` repos.
- **AUR is not part of the official trust chain** — AUR packages are
  community-maintained; rely on signed official builds for trust.
- A SOL Store (if implemented) hides package implementation details behind
  `pacman`/AUR as the real delivery mechanism (PRD §30, §41 #15).
- Flatpak sandbox is **deferred** (PRD §31, §41 #12) — MITM for sandbox
  strategy under evaluation; not an MVP blocker.

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

- Packaging continues under `packaging/arch/` targeting the three repos.
- Flatpak remains supported only as a third-party app runtime (§4.5/§6), not a
  first-class SOL distribution channel.
- The compositor never needs to maintain an XWayland path — a simplification
  vs a mixed-protocol compositor.

## Related

- PRD §7 (Package Architecture), §30 (Distribution), §31 (Security Model),
  §4.2 (Wayland Native), §19.1 (consistency boundaries); ADR-0007 (IME);
  ADR-0001 (monorepo for packaging).
