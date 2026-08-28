# Historical SOL Wayland Protocol Matrix

> **Status:** Retired migration evidence; non-normative since 2026-08-28.
> **Last audited:** 2026-08-24 against the current `sol-compositor` state,
> renderer paths, Shell client, and compositor integration tests.
> **Related:** [Roadmap](../ROADMAP.md), [PRD](../PRD.md),
> [Compositor development ADR](../decisions/0005-compositor-dev-path.md), and
> [IME ADR](../decisions/0007-ime-frontend-fcitx5-engine.md).

The implementation described below has been removed from the active source
tree. SOL exposes SCP only (ADR-0028/0032); none of these globals, tests, or
compatibility claims describe the current compositor. This file is retained
solely to preserve the evidence and gaps that informed the migration.

This matrix prevents three different claims from being collapsed into one:

1. a protocol global is advertised;
2. requests/events have enough semantic handling for a real feature;
3. representative clients use the feature correctly in a rendered hardware
   session, including failure and cleanup paths.

The maturity stages S0–S5 are defined in the Roadmap. “Advertised” is a fact
about registry exposure, not a completion state. A Smithay delegate or manager
normally establishes only part of S2.

## Phase 1 required baseline

| Interface / capability | Advertised now | Current maturity | Required for M1 | Current evidence | Missing before closure |
|---|---:|---:|---:|---|---|
| `wl_compositor` / `wl_surface` | Yes | S3 narrow | Yes | Repository clients create and commit surfaces in headless tests. | Real-client lifecycle, invalid buffer/state paths, subsurface synchronization, renderer cleanup, and hardware evidence. |
| `wl_shm` | Yes | S3 narrow | Yes | Test clients and Shell commit SHM buffers. | Representative client formats, buffer release/reuse, malformed pool/buffer behavior, visible rendering, and resource-pressure tests. |
| `wl_seat` keyboard/pointer | Yes | S2 | Yes | Keyboard and pointer handles are registered; winit and udev input adapters exist. | Correct enter/leave/focus coordinates, repeat/modifiers, cursor lifecycle, device removal, multi-output coordinates, and real input tests. |
| `wl_output` | Yes | S2 | Yes | Backend configurations create output globals; udev topology fixtures exist. | Preserve/destroy exact globals on hotplug, correct logical geometry/transform/modes, surface enter/leave, client tests, and real two-output trials. |
| `zxdg_output_manager_v1` | No | S0 | Yes | Source currently constructs `OutputManagerState::new()`, not `new_with_xdg_output()`. | Advertise supported version, logical position/size/name/description, update ordering, hotplug cleanup, and client tests. |
| `xdg_wm_base` / `xdg_surface` | Yes | S2–S3 narrow | Yes | Basic configure/ack round trips pass for repository-owned clients. | Ping timeout policy, role/lifecycle errors, geometry, destruction ordering, representative toolkit tests, and visible renderer validation. |
| `xdg_toplevel` | Yes | S2 | Yes | Creation, a basic size ack path, move/resize grabs, and in-memory focus model exist. | Correct activated/unactivated, title/App ID, parent/modal behavior, min/max constraints, maximize, minimize, fullscreen, close, output targeting, and rendered geometry. |
| `xdg_popup` / `xdg_positioner` | Global through xdg-shell | S1–S2 | Yes | Handler entry points exist. | Map and render popup trees; positioner rules, constraints, nesting, grab validation, reposition tokens, dismissal, cleanup, and toolkit menu tests. |
| `zxdg_decoration_manager_v1` | No | S0 | Yes | No compositor state or delegate is registered. | Accept the CSD/SSD policy, implement negotiation and fallback, and test GTK/Qt/Electron behavior. |
| `wp_viewporter` | No | S0 | Yes | No compositor state or delegate is registered. | Implement source/destination validation and rendering; test together with fractional scale and video clients. |
| `wp_fractional_scale_manager_v1` | Yes | S2 | Yes | A headless client observes the configured preferred scale. | Pair with viewporter, track per-output surface membership, apply buffer/render transforms, validate cross-output transitions, damage, and sharpness. |
| `zwlr_layer_shell_v1` | Yes | S2 | Yes | Shell receives a configure and commits one buffer in a headless test. | Layer maps, ordering, anchors, margins, exclusive zones, keyboard interactivity, popup parenting, output selection, rendering, and disconnect cleanup. |
| `wl_data_device_manager` clipboard | Yes | S3 headless | Yes | UTF-8 selection transfer passes in an isolated session. | Real app interoperability, MIME negotiation, cancellation, large/streaming payloads, source death, focus transitions, and optional persistence policy. |
| `wl_data_device_manager` drag-and-drop | Yes | S2 or lower | Yes | Smithay data-device plumbing and DnD handler types exist. | Complete server/client send path, action negotiation, icons, enter/motion/leave/drop, cancellation, source/target death, and app-to-app file transfer. |
| `xdg_activation_v1` | No | S0 | Yes | No activation state or token policy is registered. | Authenticated token issuance, focus-stealing policy, launcher/Shell handoff, expiry/reuse rejection, and client tests. |
| `wp_presentation` | No | S0 | Yes | Frame callbacks exist, but presentation feedback is not advertised. | Clock selection, submitted/presented/discarded feedback, refresh/sequence flags, and DRM page-flip-backed timing validation. |
| `zwp_linux_dmabuf_v1` | No | S0 | Yes | The DRM renderer can use GBM formats internally, but no client dmabuf global/feedback path is registered. | Import validation, format/modifier feedback, per-surface feedback, multi-GPU path, invalid fd/plane tests, and GTK/Qt/video hardware trials. |
| `zwp_text_input_manager_v3` | Yes | S2 | Yes | Smithay manager global and delegate are registered. | Focus/enable/disable/state batching, surrounding text, content hints, cursor rectangle, reconnect, and real application tests. |
| `zwp_input_method_manager_v2` | Yes | S2 | Yes | Smithay manager global and handler entry points are registered. | Exclusive authorization, keyboard grab, popup mapping/geometry, preedit/commit/delete flow, crash recovery, and real fcitx5 integration. |
| Compositor ↔ Shell D-Bus contract | N/A | S1 | Yes | ADR-0006 selects D-Bus. | Versioned introspection/schema, compositor service, generated/shared types, Shell proxy, signals, authentication, reconnect, crash tests, and end-to-end workspace/window actions. |

## Explicit post-M1 or product-decision backlog

These interfaces are intentionally visible even when they are not Phase 1
closure gates. Their priority must be promoted when a product scenario depends
on them; absence must not be described as implicit Smithay support.

| Interface / capability | Current maturity | Earliest owner | Required decision or evidence |
|---|---:|---|---|
| `zwp_idle_inhibit_manager_v1` | S0 | Phase 5 power/session | Visibility policy, inhibitor lifecycle, client death, suspend interaction. |
| `zwp_relative_pointer_manager_v1` | S0 | Phase 5 input/compatibility | Game/3D requirement and raw-motion validation. |
| `zwp_pointer_constraints_v1` | S0 | Phase 5 input/compatibility | Lock/confine lifecycle, focus loss, region updates, escape policy. |
| Primary selection | S0 | Phase 5 data transfer | Decide product policy, then selection interoperability and cleanup tests. |
| Data-control protocol | S0 | Phase 5 clipboard/Shell | Choose ext/wlr exposure and privileged-client policy. |
| Screencopy/export path | S0 | Phase 5 portal/capture | Portal-mediated authorization, PipeWire integration, damage/cursor semantics, revocation. |
| Output management protocol | S0 | Phase 5 display settings | Choose public/privileged protocol, transactional apply/test/revert, hotplug races. |
| Session lock | S0 | Phase 5 session/security | Trusted lock ownership, surface coverage on all outputs, failure/restart rules. |
| Tablet protocol | S0 | Phase 5 input | Device/tool/pad lifecycle and hardware matrix. |
| Explicit synchronization / DRM syncobj | S0 | Phase 5/9 graphics | Driver matrix, acquire/release timeline validation, fallback policy. |
| Color management / HDR | S0 | Phase 9 graphics/material | Product target, protocol stability, fallback and accessibility policy. |
| Virtual input | S0 | Phase 5 accessibility/remote control | Capability broker authorization and synthetic-input attribution. |

## Evidence suites required for promotion

| Promotion | Minimum protocol evidence |
|---|---|
| S1 → S2 | Supported versions and semantics are documented; production dispatch exists; invalid requests and resource destruction have focused tests. |
| S2 → S3 | A real client and server cross the intended process boundary; visible or externally observable behavior and cleanup are asserted. |
| S3 → S4 | Representative external clients run on the real renderer/backend; relevant GPU, display, input, IME, portal, or Shell boundary is exercised. |
| S4 → S5 | The release compatibility/hardware matrix, negative paths, performance targets, accessibility requirements, and regression suite pass from the release commit. |

## Required M1 interoperability scenarios

The concrete client list may change with an accepted update to this file, but
the categories and behaviors may not be replaced by repository-owned fixtures:

1. GTK4, Qt6, an SDL/Wayland client, a terminal, and a browser create multiple
   toplevels and exercise popup/menu, resize, maximize, fullscreen, focus, and
   close behavior.
2. Two unrelated applications exchange text through clipboard and a file
   through drag-and-drop, including cancellation and source/target exit.
3. The Shell maps visibly, reserves the intended work area, switches workspace
   through D-Bus, crashes, restarts, and reconstructs state without disrupting
   application clients.
4. A two-output real DRM session moves a window across outputs, changes scale,
   unplugs either output, and leaves no stale output global or unreachable
   surface.
5. Chinese, Japanese, and Korean input completes through a real application,
   compositor protocols, fcitx5 transport, candidate surface, and committed
   application text.

Each evidence report must identify the exact SOL commit, kernel, Mesa, driver,
hardware, client versions, advertised protocol versions, test commands,
results, known failures, and accepted waivers.
