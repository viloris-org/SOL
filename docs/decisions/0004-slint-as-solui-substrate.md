# 4. Slint as a candidate rendering/widget substrate for SolUI

- Status: Accepted (spike pending)
- Date: 2026-08-15

## Context

Quickshell was assessed and rejected for SOL Shell (ADR-0003). Slint is a
declarative UI toolkit native to the Rust ecosystem with native rendering,
reactive property binding, a component system, and a Rust API. We need to
evaluate whether it can become the rendering substrate behind SolUI.

## Positioning

Slint differs from Quickshell in nature:

- Quickshell: solves only the shell UI and drags in a parallel Qt/QML stack.
- Slint: can serve as a general-purpose rendering and widget base for
  SolKit/SolUI, serving both Shell and first-party apps.

If adopted, the correct layering is:

```text
SolKit Application API
        ↓
SolUI semantic components + Design Tokens + SolAnimation
        ↓
Slint (rendering / widget substrate)
```

Not apps or shell programming against `.slint` and Slint APIs directly.

## Fit with the PRD

| PRD requirement | Slint candidate assessment |
|---|---|
| Rust First | Good: Rust API, declarative UI model |
| Framework First | Satisfiable: SolUI sits above Slint to provide semantic components |
| Consistency First | Satisfiable: one Slint substrate serves both Shell and first-party apps |
| Design Tokens | Must be defined by SolKit; Slint only executes |
| Interactive Motion | To-be-verified: SolAnimation drives Slint properties; interrupt, spring, velocity, gesture progress remain unproven |
| Accessibility | To-be-verified: Wayland accessibility support against the SolKit semantic layer |
| Wayland First | To-be-verified: standard toplevels work; layer-shell integration for Shell is the primary risk |

## Required validation items

Before a final decision is made, a Slint/SolUI spike must complete:

1. `sol-shell` running as a layer-shell surface (anchor, exclusive zone,
   input region, popup).
2. Behavior under fractional scaling, HiDPI, and multi-monitor.
3. Whether SolAnimation can externally interrupt/take over Slint animation
   and achieve gesture progress → UI progress.
4. Wayland accessibility integration path.
5. Renderer GPU path, frame pacing, and input latency meeting PRD §34.
6. Slint license model supporting SOL Desktop / SOL OS distribution targets.
7. Slint kept strictly behind SolUI — application code never touching Slint
   types directly.

## Consequences

- If validation passes: SolUI rendering architecture (PRD §41 item 1) is
  locked to Slint-backed SolUI; SOL does not need to build a GPU UI toolkit
  from scratch.
- If layer-shell or animation validation fails: Slint still works for
  first-party app window UI, but Shell needs a separate SolKit rendering
  path, increasing architecture cost; revisit then.
- Compositor unaffected: `sol-compositor` remains Smithay-based; Slint does
  not enter the compositor render pipeline.
