# sol-graphics

SOL's rendering abstraction. Capabilities are exposed through a uniform
interface:

```text
Renderer
├── Surface
├── Texture
├── Shadow
├── Blur
├── Liquid Glass composition plan
├── Transform
├── Color
└── Present
```

The next native rendering slice targets SCP-owned buffers directly; the
long-term role of Vulkan / wgpu is undecided (§41 #4).

## Principles

- Decoupled from the window manager / animation engine (PRD §15).
- No premature renderer rewrite for the sake of novelty.
- Slint is the candidate rendering substrate for SolUI (ADR-0004, fractal
  validation pending).

## Status

**Phase 2 abstraction foundation implemented.** Renderbuffer, Surface,
GraphicsContext, Brush, and Paint APIs are present. `plan_material` turns a
semantic Liquid Glass role into ordered, renderer-independent composition
passes and negotiates full, reduced-effects, or solid rendering against backend
capabilities. Backdrop pixels are explicitly renderer-only and are never
returned to application code.

```rust
use sol_design::{accessibility::TokenMode, material::{Material, MaterialNesting}};
use sol_graphics::{MaterialCapabilities, plan_material};

let plan = plan_material(
    Material::Control,
    MaterialNesting::Independent,
    TokenMode::dark(),
    MaterialCapabilities::full(),
);
assert!(plan.spec.samples_backdrop);
```
