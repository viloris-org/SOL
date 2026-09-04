# ADR-0028: Drop Wayland Compatibility Layer

**Status:** Proposed  
**Date:** 2026-08-26  
**Authors:** SOL Team

## Context

SOL's PRD and ADR-0027 (SCP) define a capability-based compositor protocol with explicit security boundaries. However, the current implementation maintains dual protocol support: both Wayland (via Smithay) and the native SOL Compositor Protocol (SCP).

This dual-protocol approach creates architectural tension and undermines the security model we're trying to build.

### Product Identity Crisis

There are two fundamentally different approaches to building a new desktop platform:

**The Ubuntu Model (Linux Distribution)**
- Goal: Replace existing Linux desktop while maintaining compatibility
- Ecosystem: Support 20+ years of GTK/Qt applications
- User expectation: Run Firefox, LibreOffice, GIMP out of the box
- Technical cost: Must support X11/Wayland, D-Bus, freedesktop.org specs, CSD/SSD hybrid

**The Android Model (Linux Family Platform)**
- Goal: Define a new application experience with clear boundaries
- Ecosystem: Native SDK + explicit compatibility adapters
- User expectation: Apps are "designed for" the platform
- Technical freedom: Can design security model from first principles

SOL has already committed to the Android path through multiple PRD decisions:
- Native application framework (SolKit)
- Complete security model (sol-securityd, capability tokens)
- Curated app packaging (.app bundles, app store)
- **Explicit rejection of XWayland** (PRD §4.2)

Maintaining Wayland compatibility contradicts this direction. We cannot simultaneously be a "new platform with a security model" and "compatible with arbitrary existing apps."

### Security Model Incompatibility

Wayland's design assumptions conflict with SOL's security requirements:

| Wayland assumption | SOL requirement | Conflict |
|---|---|---|
| All protocol globals visible to all clients | Capability-based protocol access | Cannot enforce per-app protocol restrictions |
| Client-side decorations are allowed | Compositor-controlled chrome (anti-phishing) | CSD clients can draw fake system UI |
| Implicit clipboard/capture access | Explicit grant per operation | Background apps can silently read clipboard |
| Layer-shell available to any client | Layer-shell only for authenticated Shell | Third-party apps can inject fake status bar |

Every Wayland protocol extension we expose creates a security hole. The only way to close these holes while maintaining Wayland compatibility is to build complex filtering/rewriting logic—which is exactly the technical debt we're trying to avoid.

### Technical Complexity

Current compositor state (`compositor/src/state.rs`, ~3275 lines) carries:

```rust
// Wayland protocol state (via Smithay)
pub compositor_state: CompositorState,
pub xdg_shell_state: XdgShellState,
pub layer_shell_state: WlrLayerShellState,
pub shm_state: ShmState,
pub seat_state: SeatState<SolState>,
pub data_device_state: DataDeviceState,
pub fractional_scale_state: FractionalScaleManagerState,
pub text_input_state: TextInputManagerState,
pub input_method_state: InputMethodManagerState,

// SOL native protocol
pub scp_state: ScpState,  // Our actual security model
```

This dual-stack architecture means:
- Every feature must be implemented twice
- Security boundaries must be enforced retroactively on Wayland
- Protocol evolution is constrained by Wayland compatibility
- Testing surface area is doubled

### Development Tooling Argument

The main argument *for* keeping Wayland: "We need to run weston-terminal and other tools during development."

**Counterargument**: This is a development convenience, not a product requirement.

Solutions:
1. **Development mode**: Run compositor in winit backend on host Wayland session
2. **Native tooling**: Write SCP-native example clients (`compositor/examples/scp-client.rs`)
3. **Temporary shim**: If absolutely needed, maintain a dev-only Wayland bridge on a separate branch

Development tooling should not dictate production architecture.

## Decision

**SOL will NOT maintain a Wayland compatibility layer.**

The compositor will support **only** the SOL Compositor Protocol (SCP). Third-party applications must adapt through the `sol-runtime` SDK, similar to how Android apps use the Android SDK rather than raw Linux syscalls.

### Explicit Non-Goals

- We will NOT support running arbitrary Wayland applications
- We will NOT maintain Smithay integration beyond development phases
- We will NOT provide a Wayland-to-SCP translation layer
- We will NOT support wlroots layer-shell for third-party apps

### Clarified Product Position

SOL is a **Linux Family OS** (like Android/Chrome OS), not a Linux distribution (like Ubuntu/Fedora):

| Dimension | Linux Distribution | SOL (Linux Family) |
|---|---|---|
| Compatibility goal | Run existing apps | Define new app model |
| Protocol | Wayland/X11 | SCP only |
| App packaging | .deb/.rpm + dependencies | .app bundle (vendored) |
| Security model | DAC + optional AppArmor | Capability-based mandatory |
| UI consistency | Toolkit-dependent | Framework-enforced |
| Update model | Package manager | Atomic image |

This aligns with existing decisions:
- ADR-0020: App bundles vendor all dependencies
- ADR-0021: Capability-based resource authorization
- ADR-0027: SCP replaces Wayland
- PRD §4.2: No XWayland

## Migration Path

### Phase 1: SCP-Only Compositor (Immediate)

**Production compositor**:
```rust
// compositor/src/state.rs
pub struct SolState {
    scp: ScpState,              // Only protocol implementation
    windows: WindowManager,
    security: SecurityCoordinator,
    outputs: Outputs,
}
```

**Development workflow**:
```bash
# Run compositor in host Wayland session (winit backend)
WAYLAND_DISPLAY=wayland-0 cargo run -p sol-compositor

# Test with native SCP clients
cargo run --example scp-client
cargo run --example popup-client
```

**Removed immediately**:
- `CompositorState`, `XdgShellState`, `WlrLayerShellState`
- All `delegate_*` macros for Wayland protocols
- Smithay handler implementations

**Retained** (protocol-agnostic):
- `window.rs` - Window management logic
- `outputs.rs` - Display management
- Rendering pipeline (winit/udev backends)

### Phase 2: Native Rendering Pipeline

Remove Smithay's rendering abstractions:
- Implement native DRM/GBM compositor without Smithay's `State` abstraction
- Direct control over renderer, allocator, and damage tracking
- SCP-aware surface lifecycle (no Wayland surface wrappers)

### Phase 3: Production Hardening

- Remove all Wayland-related dependencies from `Cargo.toml`
- Update documentation to explicitly state: "SOL does not run Wayland applications"
- Publish migration guide for third-party developers (Wayland app → SolKit app)

### Optional: Development Shim (If Needed)

If development ergonomics require running legacy tools:

```bash
# Separate development tool (NOT shipped to users)
sol-dev-bridge --wayland-socket=/tmp/sol-dev

# Runs with visible warning
┌─────────────────────────────────────────────┐
│ ⚠️  DEVELOPMENT MODE - WAYLAND SHIM ACTIVE │
│ This is a dev tool. Production SOL does    │
│ NOT support Wayland applications.          │
└─────────────────────────────────────────────┘
```

This shim:
- Lives in `tools/dev-bridge/` (separate crate)
- Is not compiled in release builds
- Shows persistent UI warning
- Logs every security boundary violation

## Consequences

### Positive

1. **Architectural simplicity**: Single protocol stack, ~60% less compositor code
2. **Security by design**: No retrofitting security onto Wayland's implicit model
3. **Clear product positioning**: Forces explicit "native app" conversation
4. **Faster iteration**: Don't maintain two protocol implementations
5. **Stronger guarantees**: Server-side decorations are mandatory, not best-effort

### Negative

1. **No legacy app support**: Cannot run existing GTK/Qt/Electron apps without porting
2. **Smaller initial ecosystem**: Developers must explicitly target SOL
3. **Development friction**: Cannot use standard Wayland debugging tools (weston-info, etc.)
4. **Perceived incompleteness**: Users coming from Ubuntu/Fedora will see this as "broken"

### Mitigations

**For third-party developers**:
- Provide comprehensive SolKit SDK (Phase 6)
- Publish detailed migration guides (GTK→SolKit, Qt→SolKit)
- Build reference applications as porting examples
- Offer `sol-runtime` compatibility shims for common cases (file pickers, etc.)

**For development ergonomics**:
- Write comprehensive SCP example clients (`examples/`)
- Build native debugging tools (`scp-inspector`, `scp-trace`)
- Document compositor testing without external Wayland apps

**For product communication**:
- Explicit messaging: "SOL is a new platform, not a Linux desktop replacement"
- Curated app store ensures quality native apps exist before launch
- Position as "premium, secure platform" rather than "Linux desktop for everyone"

## Implementation Checklist

- [ ] Finalize SCP protocol definition (protobuf schemas)
- [ ] Implement complete SCP message handling in `compositor/src/scp/`
- [ ] Rewrite integration tests to use SCP clients (`tests/sol_session.rs`)
- [ ] Remove Smithay protocol state from `SolState`
- [ ] Remove Smithay handlers and delegate macros
- [ ] Update `Cargo.toml` dependencies (remove smithay, add protobuf)
- [ ] Write native SCP example clients for testing
- [x] Document the product philosophy in `README.md` and the PRD
- [ ] Update Phase 1 roadmap acceptance criteria
- [ ] Document SCP protocol for third-party developers

## References

- ADR-0027: SOL Compositor Protocol (SCP)
- ADR-0021: Application Sandbox and Resource Authorization
- ADR-0020: Application Packaging and Distribution
- PRD §4.2: No XWayland support
- PRD §38: Development Phases
- [Wayland security issues](https://gitlab.freedesktop.org/wayland/wayland/-/issues/11)
- Android's approach: Apps use Android SDK, not raw Linux APIs
