# ADR-0023: SOL Liquid Glass material system

- **Status:** Accepted; token/component/composition contracts implemented
- **Date:** 2026-09-04
- **Target phase:** Phase 2 token foundation / Phase 4 native rendering

## Context

SOL needs a recognizable system material with the depth, continuity and
adaptive translucency shown in the product references. Treating that as “lower
the opacity and add blur” would damage legibility, multiply GPU work, expose
security-sensitive pixels and let each application invent incompatible values.

The reference set establishes three reusable patterns: a broad media overlay
with strong edge lensing, a two-choice pill selector, and compact floating
navigation/slider controls. They share one optical grammar but need different
material weight and geometry.

## Decision

`sol-design` owns semantic `Content`, `Chrome`, `Panel`, `Floating`, `Control`,
`Sidebar`, `Dock`, and `Capsule` roles. Applications never configure raw blur,
tint, saturation, shadow, grain, refraction, chromatic aberration, radius or
material animation values.

`sol-ui` provides reusable `LiquidGlassSurface`, `GlassButton`,
`GlassSegmentedControl`, `GlassToolbar`, `GlassSlider`, and
`GlassMorphMenu` components. Every component produces a token-only contract
and an accessibility-aware, renderer-neutral frame.

`sol-graphics::plan_material` produces this ordered pipeline:

1. renderer-private backdrop sample;
2. blur and saturation;
3. rim refraction/chromatic response;
4. theme tint and restrained grain;
5. inner shade, specular edge and depth shadow;
6. explicit boundary when high contrast requires one.

Backdrop textures never cross the renderer/compositor boundary. A material
request grants no capture capability and returns no pixel data to the client.

Additional rules:

- Dense content is solid by default; glass expresses navigation, control and
  functional hierarchy.
- Nested light glass shares one backdrop group. The child keeps interaction
  tint and edge response but cannot recursively sample, blur or refract.
- Reduced transparency and high contrast are solid and non-refractive. High
  contrast adds an explicit boundary.
- Material appearance changes blur, edge response and scale together with the
  critically damped `Motion::Material` token. Reduced motion removes this
  transition.
- Shared-container menus morph from the trigger's top-center anchor. Their
  geometry uses a smooth union instead of cross-fading unrelated surfaces;
  rapid reversals retain live progress and velocity.
- Tap-driven menu expansion is critically damped. The under-damped
  `Motion::Rebound` token is reserved for direct pointer release or a dragged
  surface carrying real momentum.
- Backends degrade refraction, grain and saturation first. If backdrop
  sampling or blur is unavailable, they use the solid material fallback while
  preserving geometry, content, hit targets and semantics.

## Consequences

- SOL has one consistent glass grammar across Shell and native applications.
- Component authors can reproduce the reference patterns without copying
  visual constants.
- Software/remote renderers and accessibility preferences retain a complete,
  legible interface.
- The current implementation establishes contracts and deterministic tests;
  native compositor shaders, adaptive luminance, adversarial-backdrop visual
  tests and GPU/frame-budget validation remain required before a shipping
  fidelity claim.

## Acceptance gates

- Text and icon contrast passes against busy light/dark backdrop fixtures.
- Reduced-transparency and high-contrast plans contain no backdrop sampling or
  refraction.
- Nested controls never create a second backdrop group.
- Material animation remains interruptible and starts from live presentation
  state.
- Minimum-GPU and power-saving fallbacks change no layout or interaction.
- Requesting a material never exposes backdrop pixels to application code.

## Related

- [ADR-0004](0004-slint-as-solui-substrate.md)
- [ADR-0021](0021-application-security-permissions.md)
- [ADR-0035](0035-protected-media-capture.md)
- PRD §14 (motion), §19 (design tokens), §35 (accessibility)
