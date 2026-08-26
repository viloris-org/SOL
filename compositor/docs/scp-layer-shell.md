# SCP Layer Shell

Layer Shell is a capability-restricted protocol for creating desktop chrome surfaces like panels, docks, and overlays. It's exclusively available to `sol-shell`.

## Overview

Layer Shell surfaces exist in fixed layers relative to regular application windows:
- **Background** — Desktop background/wallpaper
- **Bottom** — Below windows (desktop widgets)
- **Top** — Above windows (panels, status bars)
- **Overlay** — Above everything (notifications, system overlays)

## Capability Security

Layer Shell requires the `layer-shell` capability, which is:
- **Granted automatically** to `sol-shell` on connection
- **Denied** to all other applications
- **Token-verified** on every layer surface creation

This ensures only the system shell can create desktop chrome, preventing:
- Phishing attacks via fake panels
- UI spoofing by malicious apps
- Unauthorized screen overlays

## Protocol Flow

### 1. Connect and Verify Capability

```rust
// Client connects
ClientMessage::Connect {
    app_id: "sol-shell",
    pid: 1000,
}

// Compositor responds with granted capabilities
CompositorMessage::Connected {
    session_id: 1,
    granted_capabilities: ["layer-shell", ...],
    capability_tokens: {
        "layer-shell": [token_bytes],
        ...
    },
}
```

### 2. Create Layer Surface

```rust
// Create base surface
ClientMessage::CreateSurface { surface_id: 1 }

// Create layer surface (requires token)
ClientMessage::CreateLayerSurface {
    surface_id: 1,
    capability_token: layer_token,
    layer: LayerShellLayer::Top,
    namespace: "panel",
    output_id: None,  // or Some(output_id)
}

// Compositor configures the surface
CompositorMessage::ConfigureLayerSurface {
    layer_id: 1,
    serial: 1,
    width: 1920,   // output width
    height: 1080,  // output height
}
```

### 3. Configure Geometry

```rust
// Anchor to top edge
ClientMessage::SetLayerAnchor {
    layer_id: 1,
    top: true,
    bottom: false,
    left: true,
    right: true,
}

// Reserve space for panel (pushes windows down)
ClientMessage::SetLayerExclusiveZone {
    layer_id: 1,
    zone: 32,  // 32px reserved at top
}

// Set margins
ClientMessage::SetLayerMargin {
    layer_id: 1,
    top: 0,
    right: 0,
    bottom: 0,
    left: 0,
}

// Set desired size
ClientMessage::SetLayerSize {
    layer_id: 1,
    width: 0,   // 0 = stretch (anchored left+right)
    height: 32,
}

// Acknowledge configuration
ClientMessage::AckLayerConfigure {
    layer_id: 1,
    serial: 1,
}
```

### 4. Attach Buffer and Commit

```rust
// Same as regular surfaces
ClientMessage::AttachBuffer { ... }
ClientMessage::Commit { surface_id: 1, ... }
```

## Anchor Semantics

Anchors determine how the surface is positioned and sized:

| Anchor Combination | Behavior |
|-------------------|----------|
| `top + left + right` | Horizontal bar at top (width = output width - margins) |
| `bottom + left + right` | Horizontal bar at bottom |
| `left + top + bottom` | Vertical bar at left (height = output height - margins) |
| `right + top + bottom` | Vertical bar at right |
| `top + left` | Corner-anchored at top-left |
| None | Centered (uses explicit width/height) |

When anchored to opposite edges (e.g., `left + right`), setting `width: 0` means "stretch to fill".

## Exclusive Zones

Exclusive zones reserve space that windows won't occupy:

- **Positive value**: Reserve space on the anchored edge
  - Top-anchored panel with `zone: 32` → windows start 32px below
  - Bottom-anchored dock with `zone: 48` → windows end 48px above
- **Zero**: No reservation (overlaps with windows)
- **-1**: Don't affect window layout, but still visible

## Keyboard Interactivity

```rust
ClientMessage::SetLayerKeyboardInteractivity {
    layer_id: 1,
    interactivity: LayerKeyboardInteractivity::OnDemand,
}
```

- **None**: No keyboard input
- **Exclusive**: Layer surface takes all keyboard input
- **OnDemand**: Keyboard input when focused (default for input fields)

## Stacking Order

Surfaces are rendered in this order (bottom to top):
1. Background layer surfaces
2. Bottom layer surfaces
3. Regular application windows
4. Top layer surfaces
5. Overlay layer surfaces

Within each layer, surfaces are rendered in creation order.

## Example: Top Panel

```rust
// 1. Create and configure
CreateLayerSurface {
    layer: Top,
    namespace: "panel",
}

// 2. Position at top, full width
SetLayerAnchor { top: true, left: true, right: true }
SetLayerSize { width: 0, height: 32 }
SetLayerExclusiveZone { zone: 32 }

// 3. Render panel content
AttachBuffer { ... }
Commit { ... }
```

## Example: Floating Notification

```rust
// 1. Create overlay
CreateLayerSurface {
    layer: Overlay,
    namespace: "notification",
}

// 2. Position at top-right corner
SetLayerAnchor { top: true, right: true }
SetLayerMargin { top: 8, right: 8 }
SetLayerSize { width: 300, height: 80 }
SetLayerExclusiveZone { zone: 0 }  // Don't push windows

// 3. Render notification
AttachBuffer { ... }
Commit { ... }
```

## Implementation Status

**Completed:**
- ✅ Layer surface creation with capability verification
- ✅ Four-layer stacking (Background/Bottom/Top/Overlay)
- ✅ Anchor-based positioning
- ✅ Exclusive zone support
- ✅ Margin and size configuration
- ✅ Keyboard interactivity modes
- ✅ Per-output layer surfaces
- ✅ Comprehensive tests

**Next Steps:**
- Integration with compositor rendering pipeline
- Multi-output exclusive zone calculation
- Shell IPC for dynamic panel updates
- Example panel implementation in sol-shell

## Security Properties

1. **Capability-gated**: Only sol-shell can create layer surfaces
2. **Token-verified**: Every creation validates the capability token
3. **Audit-logged**: All layer surface operations are logged to sol-securityd
4. **Session-isolated**: Layer surfaces are destroyed when the shell disconnects
5. **Anti-spoofing**: Regular apps cannot create fake panels or overlays

This prevents common desktop security issues like fake lock screens, phishing panels, and overlay attacks.
