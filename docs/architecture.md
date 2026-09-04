# SOL Architecture

SOL is a Linux-kernel operating system. This page maps the logical layers onto
the current monorepo and the new components introduced by the OS rebaseline.
The desktop code exists today; dashed/planned responsibilities are not yet an
implementation claim.

## Logical layers → code

```text
Applications  (apps/)       Files · Terminal · Settings · third-party .app
─────────────
SolKit        (sdk/)        source SDK and language bindings
Adapters      (planned)     bundled sol-gtk · sol-qt · portal/toolkit bridges
SOL Runtime   (planned)     stable sol-runtime-* ABI + versioned IPC
─────────────
Runtime       (services/)   settings · notifications · portal · IME
Security      (planned)     sol-securityd · atomic grants · sandbox · audit
Accounts      (planned)     sol-accountsd · sol-vaultd · provider brokers
Packages      (planned)     sol-pkg · sol-packaged · sol-bundle · app store
─────────────
Shell         (shell/)      dock · launcher · global menu · status · capsule
Compositor    (compositor/) SCP state · capabilities · surfaces · transport
─────────────
System image  (foundation)  content identity · UKI/dm-verity · physical A/B placement
Boot          (foundation)  Stage-0 · sol-boot manager · independent recovery
─────────────
Upstream                    Linux · systemd · Mesa · PipeWire · drivers · etc.
```

The target native contracts are described in [OS Platform Definition](os-platform.md).

## Executable and data lifecycle

```text
UEFI Secure Boot
  ├─ independent platform/external recovery
  └─ stable Stage-0
       ├─ retained/trial sol-boot managers
       └─ automatic platform recovery
                    ↓
       signed deployment identity
                    ↓ physical A/B selection
          complete UKI + dm-verity root
                    ↓ authenticated health gates
          separate mutable user/machine data
```

Before promotion, functional rollback selects the retained deployment and then
resolves the newest retained app bundle compatible with that deployment's
runtime descriptor.
Application rollback selects a previous compatible bundle hash. Neither
operation rewinds user data. A security rollback below the trusted epoch is
rejected, and irreversible shared-data migration waits until the rollback
barrier or uses a compatible snapshot/versioning contract.

## Boundary rules

| Boundary | Rule | Basis |
|---|---|---|
| Firmware → boot | Stable Stage-0 selects retained/trial managers; platform recovery remains independently firmware-addressable | ADR-0019, ADR-0026 |
| Boot display → UKI/DRM | Optional static drawing uses the current GOP mode without EDID or `SetMode()`; Linux owns native display policy | ADR-0026 |
| Boot → deployment | A signed content identity binds the complete UKI, dm-verity root, runtime, generation, key epoch, and security version independently of A/B placement | ADR-0019, ADR-0026 |
| System image → mutable state | Executable system content is read-only and versioned; user/machine data is outside the slots | ADR-0019 |
| Repository → install | Only `sol-packaged` stages verified manager/recovery/deployment/app transactions; Stage-0 and `sol-boot` independently activate their layers | ADR-0019, ADR-0020, ADR-0026 |
| App bundle → dependencies | A `.app` vendors all non-SOL userspace dependencies; private libraries never satisfy another app | ADR-0020 |
| App → SOL Runtime | Major + minimum contract revision + required features select the first non-revoked compatible hash in the preferred release's recorded fallback chain | ADR-0020 |
| App release → grants | App ID + verified publisher lineage is durable; bundle/process generations bind live handles; uninstall or lineage discontinuity inherits nothing | ADR-0021 |
| App → resources | Authenticated identity + declaration + explicit minimum-scope grant produces a fresh sandbox/lease; `sol-securityd` coordinates grant + audit + participant commit | ADR-0021 |
| App → accounts | Apps receive opaque handles and generation-fenced leases; account/vault prepared state is unusable without `sol-securityd` commit proof | ADR-0022 |
| GTK/Qt → SOL | Toolkit/runtime/plugins stay private in `.app`; bundled adapters use stable SOL ABI/IPC and cannot inject host libraries | ADR-0024 |
| App → global chrome | Menus, tray/status, badges, and live activities are authenticated declarative records; Shell owns rendering/input and brokers own privacy truth | ADR-0025 |
| Compositor ↔ backend | `ScpState` owns client protocol state; native renderer/input/output backends only drive it | ADR-0028, ADR-0032 |
| Compositor ↔ Shell | Separate processes over typed IPC; a shell crash never kills the compositor | PRD §11, ADR-0006 |
| Display → capture | Capture uses a separate compositor pass that replaces broker-verified protected surfaces before reading their buffers; display scanout is never a recording source | ADR-0035 |
| Audio → capture | `sol-audiod` builds a capture-only mix from allowed playback nodes; protected nodes and physical sink monitors are never recording inputs | ADR-0035 |
| App → SolKit → renderer | Apps and Shell never touch a renderer/Slint directly; `sol-ui` owns semantic components and `sol-design` owns visual parameters | PRD §19.1 |
| Monorepo crates | Each crate can eventually split out; no hidden coupling across public/private boundaries | ADR-0001, ADR-0017 |

## Application execution flow

```text
Launcher requests AppId
        ↓
sol-packaged reads authenticated runtime descriptor
        ↓
selects first non-revoked compatible hash in recorded fallback chain
        ↓ (or explicit unavailable state)
sol-securityd loads declaration + explicit grants + policy
        ↓
sandbox is constructed before entrypoint execution
        ↓
application uses SolKit / sol-runtime-N
        ↓
protected operation → typed portal/broker request
        ↓
policy denies or invokes trusted Shell consent UI
        ↓
grant + audit + scoped handle commit atomically
```

An app cannot gain authority by editing its bundle, calling a private D-Bus
name, or shipping its own runtime. Capability enforcement lives outside the app
process.

An OS rollback repeats application compatibility resolution. It may activate an
older retained app bundle, but does not rewind app data. A missing compatible
bundle makes only that application unavailable; it cannot block system boot.

For an account-scoped grant, `sol-securityd` coordinates the distributed
participants:

```text
sol-securityd transaction id
       ├── prepare association ──→ sol-accountsd (not enumerable)
       └── prepare lease ────────→ sol-vaultd    (not usable)
                    ↓
grant + audit + receipts + authorization generation commit
                    ↓ verifiable proof
participants activate idempotently; stale generations remain denied
```

## Key current compositor state

- `compositor/src/scp/state.rs` — `ScpState`: authenticated native protocol
  state, capability checks, and object routing.
- `compositor/src/main.rs` — SCP service lifetime and socket ownership.
- `compositor/examples/scp-client.rs` — reference native client.
- `compositor/tests/scp_session.rs` — end-to-end native session test.

Every feature that grows the compositor belongs in `ScpState` or a focused
`scp/` module, not in `main.rs`.

## Rendering and framework stack

```text
application source
   ↓
SolKit language binding / semantic components
   ↓
stable sol-runtime-N ABI and versioned service IPC
   ↓
private Slint adapter + sol-graphics
   ↓
SCP surface → SOL Compositor
```

The framework boundary is deliberately not Rust ABI. Rust applications consume
safe bindings; installed applications rely on the stable lower-level contract.

Non-native applications take a parallel path:

```text
GTK / Qt / Electron / Flutter application
   ↓
private toolkit runtime + toolkit-matching bundled SOL adapter
   ↓
stable portals / accounts / accessibility / lifecycle ABI or IPC
   ↓
SCP surface
   ↓
SOL Compositor
```

Adapter absence or failure makes the application unavailable; SOL does not
expose an implicit compatibility socket or broaden the application's authority.

## Shell spatial and live-activity flow

```text
focused SCP surface → authenticated AppId → atomic global-menu snapshot

app live activity ── declared capability + explicit grant ──┐
                                                            ├→ Live Capsule
broker media/privacy lease ─────────────────────────────────┘
                                                            ↓
                              Shell-rendered typed actions / Stop / Revoke
                                                            ↓
                                 owning app or capability broker session
```

The upper-left app menu and upper-right status/capsule zones are never
application-rendered. Privacy activities remain visible even if the app hangs,
and stopping them targets the underlying broker session rather than merely
dismissing UI. See [Shell Experience](shell-experience.md).

## See also

- [OS Platform Definition](os-platform.md)
- [PRD](PRD.md)
- [Decision log](decisions/README.md)
- [Roadmap](ROADMAP.md)
