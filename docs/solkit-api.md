# SolKit API Map

This is the current source-level map for the unpublished SolKit workspace. It
is a navigation aid for the public crates, not a stability promise. The
post-v0.1 compatibility policy is recorded in
[ADR-0017](decisions/0017-solkit-stability-tiers.md).

## Public crates

| Crate | Use it for | Start with |
|---|---|---|
| `sol-app` | Application identity, lifecycle, windows, and commands | `AppId`, `App`, `AppWindow`, `CommandRegistry` |
| `sol-design` | Semantic colors, spacing, typography, materials, motion, and accessibility modes | `Color`, `Spacing`, `Typography`, `Motion`, `TokenMode` |
| `sol-ui` | Renderer-neutral semantic controls and interaction trees | `Button`, `TextField`, `Toolbar`, `TabBar`, `InteractionTree` |
| `sol-graphics` | Renderer-independent drawing contracts | `Renderbuffer`, `Surface`, `GraphicsContext`, `Paint` |
| `sol-animation` | Semantic motion and interruptible transitions | `AnimationEffect`, `AnimationContext`, `InterruptibleAnimation` |

Normal applications should depend on these crates together. Application code
should express semantic roles and token values rather than importing Slint,
Wayland, Smithay, or GPU types.

## Restricted crate

`sol-system` is a restricted capability boundary. It contains typed settings,
notifications, and system-action authorization. A caller must use a validated
`AppId` and a closed action catalog; there is no arbitrary command or shell
escape hatch.

The system-action flow is:

```text
SystemActionRequest
    -> SystemActionApi
    -> PermissionPolicy / PermissionStore
    -> ActionAuthorization, Denied, or UserConsentRequest
```

`FilePermissionStore` and `FileActionAuditStore` are daemon-owned persistence
implementations. They do not replace a trusted consent UI, polkit policy, or a
concrete system adapter.

## Example shape

```rust
use sol_app::{App, AppId, AppWindow};

let id = AppId::parse("com.example.notes")?;
let mut app = App::new(id);
app.add_window(AppWindow::new("Notes"));
app.start()?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Generate full rustdoc

The complete item-level reference is generated from the source so signatures
cannot drift from this map:

```bash
cargo doc --workspace --no-deps --locked
```

The generated documentation is the authoritative detail for method arguments,
error variants, and feature flags. Current crates are local workspace crates;
registry publication and external-consumer compatibility tests remain open
Phase 6 work.

## Validate the public boundary

Run `scripts/validate-solkit-public-api.sh` before changing a public crate. It
checks the five Public-tier packages remain version-aligned, unpublished until
the release gate, library targets, and free of dependencies on compositor,
shell, session, or service implementation crates.
