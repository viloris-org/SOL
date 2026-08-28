# SolKit showcase

This is a compact Phase 2 acceptance example. It builds one app exclusively
through public SolKit crates:

```text
sol-app         lifecycle + command
sol-ui          semantic components, keyboard behavior, accessibility tree
sol-design      theme, high-contrast, reduced-motion, text-scale tokens
sol-animation   semantic animation effect
sol-graphics    renderer-neutral presentation contract
```

It does not name a concrete renderer, Wayland client, Slint, Smithay, winit,
or GPU API.

## Run headlessly

```bash
cargo run -p solkit-showcase
cargo test -p solkit-showcase
```

The CLI prints a deterministic report after it starts the app, executes
`file.open`, traverses and activates the primary button, edits a text field,
selects a tab, and resolves reduced motion through the active token mode.

## Run natively

```bash
cargo run -p solkit-showcase --features native
```

The `native` feature uses SolUI's private adapter; the example itself still
does not reference any backend type. A production window requires the pending
SCP application-surface adapter. It is intentionally not an AT-SPI/screen-reader, GPU-pacing,
multi-output, or input-latency certification; those remain Phase 2 closure
environment tests.
