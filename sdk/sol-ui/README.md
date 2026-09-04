# sol-ui

SOL native UI components. SOL apps are written against a **semantic API**
(`Button(role: .Primary)`); visual properties (color / spacing / radius /
motion) come from `sol-design` tokens, never hand-written bare hex / ms /
f32. (PRD §18, §19.1 Consistency First.)

## Positioning

ADR-0004 settles Slint as the native rendering/widget adapter. SolUI owns
retained semantic state and projects it into a private reactive Slint tree;
applications never see Slint types. The Phase 2 spike has repeatable headless
coverage for token projection, fractional-scale conversion, and external
`sol-animation` gesture progress. GPU performance, accessibility, real
multi-output, and distribution licensing remain explicit follow-ups in the
ADR.

```
SolKit Application API
        ↓
SolUI semantic components + Design Tokens + SolAnimation
        ↓
Slint (private rendering / widget adapter)
```

App code never programs against `.slint` or Slint APIs directly.

## Responsibilities

- Layout (`HStack` / `VStack` semantic layout)
- Components (Button / Toolbar / TextField / Tab plus Liquid Glass surfaces,
  buttons, segmented controls, floating toolbars, sliders, and shared-container
  morph menus)
- Typography, theme, input, focus, animation (delegated to `sol-animation`)
- Accessibility (semantic tree, keyboard navigation, reduced motion)
- Rendering integration (decoupled from the underlying renderer, §19.1)

## Status

**Phase 2 architecture spike complete.** `native` compiles the private Slint
adapter; the default feature set keeps deterministic semantic tests headless
and renderer-independent. SCP surface negotiation remains outside SolUI and is
owned by the application runtime or shell. SolUI owns keyboard focus traversal,
standard activation/selection/text-editing behavior, and a renderer-neutral
accessibility semantic tree. Real AT-SPI transport remains integration work.

## Liquid Glass components

The component API mirrors the supplied visual references without exposing raw
optical values:

```rust
use sol_ui::{
    GlassButton, GlassMenuItem, GlassMorphMenu, GlassSegment, GlassSegmentedControl,
    GlassSlider, GlassToolbar, GlassToolbarItem, MaterializedComponent,
};

let modes = GlassSegmentedControl::new("Capture type")
    .segment(GlassSegment::new("video", "Video"))
    .segment(GlassSegment::new("photo", "Photo"));
let toolbar = GlassToolbar::new()
    .item(GlassToolbarItem::Button(GlassButton::new("Previous")))
    .item(GlassToolbarItem::Button(GlassButton::new("Play")));
let hue = GlassSlider::new("Hue", 62).hue_track();
let account = GlassMorphMenu::new("Account", "Open account menu")
    .item(GlassMenuItem::new("profile", "My Profile"))
    .item(GlassMenuItem::new("settings", "Settings"))
    .item(GlassMenuItem::new("logout", "Log Out"));

assert_eq!(modes.selected_id(), Some("video"));
assert!(toolbar.items.len() == 2);
assert!(hue.material_tokens().snapshot().contains("material=Control"));
assert!(account.material_tokens().snapshot().contains("motion=Morph"));
```

Selected indicators and toolbar buttons automatically consolidate into their
parent glass backdrop group. Arrow keys switch segmented choices and adjust
slider percentages through the same semantic/accessibility tree as existing
SolUI controls.

`GlassMorphMenuController` expands one shared smooth-union surface from its
circular trigger, supports direct gesture takeover, retargets rapid reversals
from the live presentation value, and hands release velocity to the rebound
spring. Pointer-down compression is immediate; keyboard activation does not
add decorative bounce. Reduced motion snaps geometry and requests a 160ms
content cross-fade.

## Consistency iron rules (PRD §19.1)

- A `sol-ui` / first-party app commit **containing bare hex, bare ms, or bare
  f32 visual parameters is rejected at merge time**.
- Every new component must pass Design Review before entering `sol-ui`.
- `sol-files`, the most complex first app, carries the polish baseline.
- Consistency is verified by `golden-snapshot` CI: rendered output may contain
  only token values.
