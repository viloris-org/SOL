# 3. SOL Shell not based on Quickshell

- Status: Accepted
- Date: 2026-08-15

## Context

Quickshell is a Wayland-shell framework built on Qt6/QML, aimed at building
layer-shell UI (bar, dock, notification, launcher) for external compositors
like Hyprland and niri. It is not a compositor itself.

The PRD has locked SOL's tech stack to Rust + Smithay + SolKit/SolUI and
requires the Shell to be a separate process joined to the compositor by
typed IPC.

## Evaluation

| Dimension | Quickshell as a Shell substrate | SOL requirement |
|---|---|---|
| UI stack | Qt6 / QML | SolKit / SolUI (Rust) |
| Compositor relationship | Generic shell for external compositors | Deeply integrated with our own Smithay compositor |
| Framework First | Cannot dogfood SolKit | Shell must verify SolKit's layer-shell/overlay capability first |
| Design tokens | Handled in QML side | Same token/motion system as SolUI |
| Animation | Qt/QML animation model | SolKit + compositor's unified interruptible/gesture-driven animation |
| Typed IPC | QML-side IPC re-wrap | End-to-end type-safe Rust |
| Long-term ownership | Core shell depends on an external framework's cadence | Shell is a first-party SOL asset |

## Decision

Quickshell is **not** used as the SOL Shell substrate, and does not enter the
monorepo's formal dependency tree.

Permitted exception: while SolKit/SolUI is not yet mature, Quickshell may be
used as a **standalone UX/interaction prototype** for validating visual and
information-architecture decisions, provided:

- It does not enter the `sol-shell` crate or the workspace dependency tree.
- It does not carry any final implementation logic.
- A clear sunset point is set.
- Interaction conclusions are lifted into SolKit/SolUI, not ported verbatim.

## Consequences

- Phase 4 Shell UI must stand on SolKit's layer-shell client capability.
- Early shell-interaction validation will be slower than using Quickshell
  directly; the trade-off is a single UI stack and SolKit dogfooding.
- We can still reference Quickshell's mature shell behaviors as a product
  reference.
