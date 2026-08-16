# 15. System overlays and layer-shell popup contract

- Status: Accepted (contract and headless fixture)
- Date: 2026-08-16
- Target phase: Phase 4

## Context

`sol-shell --once` already proves a real top-bar layer-shell surface through
the Smithay `sol_session` round trip. System UI also needs OSDs, menus,
popovers, and modal dialogs with consistent placement, input, focus, keyboard,
accessibility, scale, and reduced-motion rules. Wayland protocol types must
not leak into SolUI or applications.

## Decision

`sol-shell` owns a renderer-neutral `overlay` contract. A request has a stable
id, semantic role, output id, anchor, logical size, exclusive zone, input
region, dismiss policy, and accessible label. It is validated before a native
host is called, then resolved against compositor output facts into a
`LayerShellSurfaceContract`.

| Role | Layer | Exclusive zone | Input / focus | Scrim |
|---|---|---|---|---|
| Panel | top | auto or fixed | interactive | no |
| OSD | overlay | none | pass-through; no focus | no |
| Menu / popover | overlay | none | interactive; stack top gets focus | no |
| Modal | overlay | none | interactive; stack top gets focus | yes |

`OverlayManager` owns stack, Escape/dismiss/focus-restoration policy.
Interactive surfaces delegate traversal and activation to SolUI's
`InteractionTree`; the exported semantic tree includes modal scrims. Motion is
resolved only from `sol-design::TokenMode`, so reduced motion is zero-duration
without a shell-local timing literal. Output identity, logical extent,
fractional scale, and physical-size conversion are explicit contract facts.

## Evidence and limits

`cargo test -p sol-shell` drives `HeadlessLayerShellFixture`, which hands each
resolved contract to SolUI's `FixtureSurfaceHost` / `RecordingRenderer`. It
asserts output-specific bottom-right placement at 1.25 scale; role, layer,
anchor, exclusive-zone and input policy; popup Tab/Escape lifecycle and focus
restoration; pointer-transparent OSDs; modal scrim accessibility; and reduced
motion/high contrast tokens.

This is an integration fixture, not a claim that CI created a native popup.
`sol_session` remains evidence for the real base top-bar layer-shell lifecycle.
A native adapter still needs transient create/configure/close ordering, pointer
grabs/input regions, and transient parenting against a real compositor.

No physical multi-monitor, GPU/frame-pacing, or Wayland screen-reader/AT-SPI
session is available here. Those require field validation, as do real IME
candidate-window and recording-indicator producers.

## Consequences

- SolUI and applications receive no `zwlr_layer_shell` or renderer API.
- Future native hosts consume one typed contract rather than reimplement role
  policy per overlay.
- This enables, but does not deliver, notification, quick-settings, IME, and
  recording UI.
