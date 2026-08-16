# 4. Slint-backed retained/reactive rendering for SolUI

- Status: Accepted — Phase 2 architecture spike completed
- Date: 2026-08-16

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

The chosen layering is:

```text
SolKit Application API
        ↓
SolUI semantic components + Design Tokens + SolAnimation
        ↓
Slint (rendering / widget substrate)
```

`sol-ui` owns retained semantic component state and a renderer-neutral
`SurfaceHost` contract. The private Slint adapter projects that state into a
reactive Slint component tree. Application code must not program against
`.slint` or Slint APIs directly.

There is one declarative source of truth: SolUI semantic state. Slint
properties are a projection, not a second app-facing UI model. This preserves
SOL ownership of focus, accessibility semantics, and `sol-animation`
interruption while using Slint for native widget layout and rendering.

This settles PRD §41 decisions #1 and #2: the rendering architecture is
retained, renderer-neutral SolUI state with a private Slint adapter; the UI
model is reactive/declarative at the API boundary, backed by that retained
state. Immediate-mode drawing and a public Slint model are rejected.

## Fit with the PRD

| PRD requirement | Assessment |
|---|---|
| Rust First | Satisfied: public contract and adapter are Rust. |
| Framework First | Satisfied: SolUI semantics sit above Slint. |
| Consistency First | Satisfied at the adapter boundary: components resolve `sol-design` tokens before projection. |
| Design Tokens | SolKit owns token resolution; Slint only executes the resolved values. |
| Interactive Motion | Validated at adapter boundary: `ButtonController` uses `sol-animation::InterruptibleAnimation` and externally overwrites Slint progress while preserving velocity. |
| Accessibility | SolUI now supplies a retained renderer-neutral role/state tree plus uniform keyboard focus/editing; a real Wayland screen-reader/AT-SPI bridge remains unvalidated. |
| Wayland First | Standard-window adapter compiles for Wayland/winit. Shell layer surfaces remain owned by `sol-shell`, not Slint. |

## Phase 2 spike and evidence

The repeatable implementation lives in `sdk/sol-ui`:

- `cargo test -p sol-ui` verifies retained semantic state, token resolution,
  fractional scale conversion, frame scheduling, and gesture takeover using a
  deterministic `FixtureSurfaceHost` / `RecordingRenderer`.
- `cargo test -p sol-ui --features native` additionally builds a real Slint
  component on Slint's headless software window and asserts that SOL tokens,
  label data, and externally driven progress reach it.
- `cargo check -p sol-ui --features native` compiles the production Wayland
  winit adapter feature (`backend-winit-wayland`) without exposing Slint types
  in the public SolUI API.

| Required item | Result | Evidence / limit |
|---|---|---|
| Retained vs reactive/declarative model | **Settled** | Retained `ButtonController` is the semantic owner; its state is reactively projected to Slint. |
| SolUI rendering architecture | **Settled** | Slint is the native widget/rendering adapter behind renderer-neutral SolUI state and `SurfaceHost`. |
| External animation takeover | **Validated in a headless fixture** | `slint_adapter_receives_tokens_and_external_gesture_progress` changes private Slint progress through `sol-animation`. Spring integration and frame-time measurements remain future work. |
| Layer-shell anchor / exclusive zone / configure / frame | **Validated at host boundary** | `cargo test -p sol-compositor --test sol_session` runs `sol-shell --once` through the real layer-shell round-trip. The Slint adapter deliberately is not a layer-shell client; Phase 4 will host SolUI through `SurfaceHost`. Popups and input regions are not yet covered. |
| Fractional scale / HiDPI / multi-monitor | **Contract fixture only** | `LogicalSize::physical_pixels(1.25)` is deterministic and `SurfaceHost` carries scale, but no physical multi-output test is available in this headless environment. |
| Accessibility | **Semantic core validated; platform bridge not yet validated** | `InteractionTree` covers focus traversal, activation, tab selection, text editing, and renderer-neutral accessibility state. A Wayland screen-reader/AT-SPI bridge still needs a real assistive-technology session. |
| GPU path, pacing, input latency | **Not yet validated** | The spike uses Slint's software renderer for reproducible headless tests. GPU renderer selection, damage/frame pacing, and PRD §34 measurements require a real Wayland/GPU session. |
| License / distribution | **Not yet cleared for distribution** | Slint 1.13.1 advertises GPL-3.0-only, royalty-free, and software license alternatives. A distribution license choice/review is required before shipping SOL binaries. |
| Slint containment | **Validated by API boundary** | `slint` is an optional private adapter dependency; public `sol-ui` APIs exchange only SOL types. |

The feature and fixture make unavailable system validation explicit instead of
making a hardware claim from a unit test. Phase 4 must provide a `SurfaceHost`
over its already-proven layer-shell surface and add real output, popup,
input-region, accessibility, and performance integration coverage.

## Consequences

- SOL does not build a general-purpose GPU widget toolkit at this stage.
- Shell remains a distinct Wayland surface host, preserving ADR-0003/0006's
  process and protocol boundaries instead of pretending a normal Slint window
  is a layer-shell surface.
- Revisit the backend only if the outstanding hardware performance,
  accessibility, or distribution-license gates fail. Those gates do not reopen
  decisions #1/#2; they determine whether a future adapter replacement is
  necessary.
- Compositor unaffected: `sol-compositor` remains Smithay-based; Slint does
  not enter the compositor render pipeline.
