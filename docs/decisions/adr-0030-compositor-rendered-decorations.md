# ADR-0030: Compositor-Rendered Window Decorations and IME Popups

## Status
Accepted

## Context

SOL is a Linux Family OS that prioritizes visual consistency, security, and platform cohesion. Two specific rendering responsibilities need clear ownership between the compositor and applications:

1. **Window decorations** (title bars, close buttons, shadows, rounded corners)
2. **IME candidate windows** (input method selection popups)

The Wayland ecosystem typically uses Client-Side Decorations (CSD), where each application renders its own window frame. This leads to:
- Inconsistent visual appearance across applications
- Duplicated rendering code in every toolkit
- Security issues (applications can fake system UI elements)
- Difficulty enforcing platform design tokens

## Decision

SOL adopts a **Server-Side Decoration (SSD) + Compositor Rendering** model, similar to macOS WindowServer:

### 1. Window Decorations
The compositor owns and renders all window decorations:
- Title bars (height, typography, color from `sol-design`)
- Window control buttons (close, minimize, maximize)
- Shadows and backdrop blur effects
- Corner rounding
- Focus indicators

**Applications are responsible only for their content area.** They submit buffers containing rendered content but have no control over the window frame.

### 2. IME Candidate Windows
The compositor directly renders IME candidate popups:
- `sol-ime` service provides candidate data via D-Bus IPC
- Compositor receives cursor position from application via SCP
- Compositor renders candidate window using `sol-design` tokens
- No separate surface/window creation by IME process

### Architecture

```
┌─────────────────────────────────────────────────┐
│  Application Process                            │
│  ┌─────────────────────────────┐               │
│  │ Content Area                │               │
│  │ (app renders via            │               │
│  │  Skia/wgpu/Vulkan)          │               │
│  │                             │               │
│  └─────────────────────────────┘               │
│         │ SCP: submit buffer                    │
│         │ SCP: set_title("My App")              │
│         │ SCP: text_input_cursor_rect(x,y,w,h) │
└─────────┼─────────────────────────────────────┘
          ↓
┌─────────────────────────────────────────────────┐
│  Compositor (sol-compositor)                    │
│  ┌──────────────────────────────────┐          │
│  │ WindowDecoration::render()       │          │
│  │  - Title bar (sol-design tokens) │          │
│  │  - Control buttons               │          │
│  │  - Shadow, corner radius         │          │
│  │  - Composite app buffer          │          │
│  └──────────────────────────────────┘          │
│  ┌──────────────────────────────────┐          │
│  │ ImeCandidateWindow::render()     │          │
│  │  - Candidate list                │          │
│  │  - Page indicators               │          │
│  │  - Selection highlight           │          │
│  └──────────────────────────────────┘          │
└─────────┬───────────────────────────────────────┘
          │ D-Bus IPC
┌─────────┴───────────────────────────────────────┐
│  sol-ime Service                                │
│  (fcitx5 engine bridge)                         │
└─────────────────────────────────────────────────┘
```

## Rationale

### Visual Consistency
- All window decorations use the same `sol-design` tokens
- Title bar height, button positions, shadows are uniform across all apps
- Theme changes applied instantly by compositor without app involvement

### Security
- Applications cannot fake close buttons (phishing prevention)
- Applications cannot hide title bar (user always sees app_id)
- System can add security indicators to title bar (e.g., "Screen Recording")

### Code Simplicity
**Wayland CSD approach:**
```rust
// Every app needs this
fn draw_title_bar() { /* 100+ lines */ }
fn handle_close_button() { /* event handling */ }
fn draw_shadow() { /* blur kernel */ }
```

**SOL SSD approach:**
```rust
// App code
canvas.draw_text("Hello");
// Title bar? Compositor handles it
```

### IME Specific Benefits
- No need for `zwp_input_popup` or equivalent protocol complexity
- IME service has no window creation capability (reduced attack surface)
- Perfect visual consistency with system theme
- Lower latency (one D-Bus call vs. multiple Wayland protocol round-trips)

## Implementation

### Phase 1: Basic Window Decorations
```rust
// compositor/src/window_decoration.rs
pub struct WindowDecoration {
    title_bar_height: u32,    // from sol-design
    corner_radius: f32,       // from sol-design
    shadow: Shadow,           // from sol-design
}

impl WindowDecoration {
    pub fn render(
        &self,
        window: &Window,
        content_buffer: &Buffer,
        renderer: &mut Renderer
    ) {
        // 1. Draw window background with corner radius
        // 2. Draw title bar with window.title
        // 3. Draw control buttons (close/minimize/maximize)
        // 4. Composite content_buffer into content area
        // 5. Apply drop shadow
    }
}
```

### Phase 1: IME Candidate Rendering
```rust
// compositor/src/ime.rs
pub struct ImeCandidateWindow {
    pub candidates: Vec<ImeCandidate>,
    pub cursor_position: (i32, i32),
    pub selected_index: usize,
}

impl ImeCandidateWindow {
    pub fn render(&self, renderer: &mut Renderer) {
        // Use sol-design tokens: surface, text, accent
        // Render candidate list at cursor_position
    }
}
```

### SCP Protocol Changes
```rust
// compositor/src/scp/toplevel.rs
// Applications only set metadata, not decoration
request.set_title(title: String);
request.set_app_id(app_id: String);
request.set_minimized();
request.set_maximized();

// compositor/src/scp/text_input.rs
// Applications report cursor position for IME
request.set_cursor_rectangle(x: i32, y: i32, width: i32, height: i32);
```

### SDK Changes
```rust
// sdk/sol-app/src/window.rs
pub struct Window {
    content_surface: ContentSurface,  // Only content area
}

impl Window {
    pub fn set_title(&mut self, title: &str);
    // No APIs for drawing title bars or decorations
}
```

## Consequences

### Positive
- **Perfect visual consistency** across all applications
- **Simpler application code** (no decoration rendering)
- **Better security** (system controls all window chrome)
- **Easier theming** (compositor applies changes immediately)
- **Smaller app binaries** (no decoration rendering code)
- **Platform differentiation** (clear SOL visual identity)

### Negative
- **Less flexibility** for applications wanting custom decorations
  - Mitigation: Support "borderless" mode for special cases (games, kiosks)
- **Compositor complexity** increases
  - Mitigation: Well-contained in `WindowDecoration` module, uses `sol-design`
- **Custom title bar UI** (like macOS unified toolbar) needs special protocol
  - Mitigation: Defer to Phase 2, most apps don't need this

### Neutral
- Applications still render content area with full freedom (Vulkan/wgpu/Skia)
- High-performance apps (games) unaffected, they already use borderless fullscreen

## Comparison with Other Platforms

| Platform | Window Decorations | IME Popups |
|----------|-------------------|-----------|
| **macOS** | WindowServer renders | System renders |
| **Windows** | DWM renders | System renders |
| **GNOME/Wayland** | Apps render (CSD) | IME creates surface |
| **KDE/Wayland** | Mixed (SSD option) | IME creates surface |
| **Android** | System UI (status bar) | System renders |
| **SOL** | **Compositor renders** | **Compositor renders** |

## Related Decisions
- ADR-0028: No Wayland compatibility layer (enables this architecture)
- ADR-0014: IME architecture (defines sol-ime service)
- Sol-design tokens system (provides visual parameters)

## References
- macOS WindowServer architecture
- Android WindowManager and IME framework
- Wayland CSD vs SSD debates
- `sol-design` token specifications
