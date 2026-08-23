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
Compat        (planned)     bundled sol-gtk · sol-qt · portal/toolkit adapters
SOL Runtime   (planned)     stable sol-runtime-* ABI + versioned IPC
─────────────
Runtime       (services/)   settings · notifications · portal · IME
Security      (planned)     sol-securityd · atomic grants · sandbox · audit
Accounts      (planned)     sol-accountsd · sol-vaultd · provider brokers
Packages      (planned)     sol-pkg · sol-packaged · sol-bundle · app store
─────────────
Shell         (shell/)      dock · launcher · global menu · status · capsule
Compositor    (compositor/) Smithay · Wayland · scene · WM · input
─────────────
System image  (planned)     sol-image · read-only A/B slots · recovery
Boot          (planned)     signed sol-boot UEFI executable and slot policy
─────────────
Upstream                    Linux · systemd · Mesa · PipeWire · drivers · etc.
```

Arch packaging under `packaging/arch/` is transitional build/bootstrap work.
It is not the target installed-system package authority. The target native
contracts are described in [OS Platform Definition](os-platform.md).

## Executable and data lifecycle

```text
signed repository metadata
          ↓ verify
 sol-packaged transaction
     ┌────────────┬───────────────────┐
     ↓            ↓                   ↓
inactive EFI/  inactive deployment  content-addressed .app
recovery copy  kernel+initrd+root       ↓ compatible switch
     ↓ trial       ↓ next boot       sandboxed app process
     └────────→ sol-boot verification ←──────┘
                       ↓
          separate mutable user/machine data
```

System rollback selects a previous deployment and then resolves the newest
retained app bundle compatible with that deployment's runtime descriptor.
Application rollback selects a previous compatible bundle hash. Neither
operation rewinds user data.

## Boundary rules

| Boundary | Rule | Basis |
|---|---|---|
| Firmware → boot | Redundant `sol-boot`/recovery copies use trial activation and a retained firmware-visible fallback | ADR-0019 |
| Boot → deployment | A signed manifest binds a slot's kernel, initrd, root-image digest, runtime descriptors, and generation | ADR-0019 |
| System image → mutable state | Executable system content is read-only and versioned; user/machine data is outside the slots | ADR-0019 |
| Repository → install | Only `sol-packaged` commits verified boot/recovery/system/app transactions; CLI and Software are clients | ADR-0019, ADR-0020 |
| App bundle → dependencies | A `.app` vendors all non-SOL userspace dependencies; private libraries never satisfy another app | ADR-0020 |
| App → SOL Runtime | Major + minimum contract revision + required features select the first non-revoked compatible hash in the preferred release's recorded fallback chain | ADR-0020 |
| App release → grants | App ID + verified publisher lineage is durable; bundle/process generations bind live handles; uninstall or lineage discontinuity inherits nothing | ADR-0021 |
| App → resources | Authenticated identity + declaration + explicit minimum-scope grant produces a fresh sandbox/lease; `sol-securityd` coordinates grant + audit + participant commit | ADR-0021 |
| App → accounts | Apps receive opaque handles and generation-fenced leases; account/vault prepared state is unusable without `sol-securityd` commit proof | ADR-0022 |
| UI → material | Components select semantic material roles; `sol-design` resolves tokens and the compositor owns protected backdrop effects/fallbacks | ADR-0023 |
| GTK/Qt → SOL | Toolkit/runtime/plugins stay private in `.app`; bundled adapters use stable SOL ABI/IPC and cannot inject host libraries or access backdrop pixels | ADR-0024 |
| App → global chrome | Menus, tray/status, badges, and live activities are authenticated declarative records; Shell owns rendering/input and brokers own privacy truth | ADR-0025 |
| Compositor ↔ backend | `SolState` owns all protocol state; backends only drive it | ADR-0005, ADR-0006 |
| Compositor ↔ Shell | Separate processes over typed IPC; a shell crash never kills the compositor | PRD §11, ADR-0006 |
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

- `compositor/src/state.rs` — `SolState`: Smithay protocol state and handlers.
- `compositor/src/main.rs` — backend event loop, rendering, client dispatch,
  frame callbacks, and sockets.
- `compositor/examples/test-client.rs` — reference client proving a round trip.
- `compositor/tests/sol_session.rs` — end-to-end session test.

Every feature that grows the compositor belongs in `SolState` (as a new
handler/state), not in `main.rs`.

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
Wayland surface → SOL Compositor
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
Wayland surface + optional semantic material-role request
   ↓
SOL Compositor (renders or safely falls back; returns no backdrop pixels)
```

Adapter absence or failure falls back to baseline Wayland/portal behavior where
the toolkit supports it; it never changes the application's authority.

## Shell spatial and live-activity flow

```text
focused Wayland surface → authenticated AppId → atomic global-menu snapshot

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
