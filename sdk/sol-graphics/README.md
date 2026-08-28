# sol-graphics

SOL's rendering abstraction. Capabilities are exposed through a uniform
interface:

```text
Renderer
├── Surface
├── Texture
├── Shadow
├── Blur
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
GraphicsContext, Brush, and Paint APIs are present. Backend rendering
integration converges alongside the SolUI spike.
