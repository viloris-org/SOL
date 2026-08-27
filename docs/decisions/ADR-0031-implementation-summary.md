# ADR-0031 Implementation Summary

**Date**: 2026-08-26  
**Status**: Phase 0 Complete

## What Was Implemented

Successfully implemented the foundation of `sol-deviced` (Device Control Plane) according to ADR-0031.

### Core Components

1. **Type System** (`src/types.rs`)
   - Three-level device graph: `PhysicalDevice` → `Function` → `Endpoint`
   - Stable identity types: `DeviceId`, `AttachmentId`, `FunctionId`, `EndpointId`, `OperationId`
   - Identity confidence hierarchy: Cryptographic > VendorProductSerial > StableProtocol > Topology > Ephemeral
   - Typed state enums: `AttachmentState`, `UsabilityState`, `ActivityState`, `TrustPolicy`
   - Complete `DeviceSnapshot` with monotonic revision tracking

2. **Reconciliation Engine** (`src/reconcile.rs`)
   - Idempotent add/update/remove transitions
   - Identity evidence resolution with confidence levels
   - Connection generation tracking for fence-based operation safety
   - Device and function graph reconciliation
   - Safe removal state machine (`quiescing` → `removed`)

3. **Backend Adapter Trait** (`src/adapters/mod.rs`)
   - Abstract `Adapter` trait for backend integration
   - `FakeAdapter` for testing without real hardware
   - Operations: `start`, `stop`, `enumerate`, `prepare_removal`, `commit_removal`, `abort_removal`

4. **D-Bus Interface** (`src/dbus/mod.rs`)
   - `org.sol.Device1` on system bus at `/org/sol/Device1`
   - Methods: `GetSnapshot`, `GetDevice`, `Authorize`, `SetTrust`, `Forget`, `Eject`, `PrepareRemoval`, `CancelOperation`, `GetOperation`
   - Operation channel for async request handling

5. **Service Core** (`src/main.rs`)
   - Service loop with reconciliation every 5 seconds
   - Operation handling via D-Bus → core channel
   - Adapter lifecycle management
   - D-Bus service registration

### Tests Passing

All reconciliation tests pass:
- ✅ Coldplug equals hotplug sequence
- ✅ Duplicate events converge to same state
- ✅ Removal fences attachment generation
- ✅ Fake adapter lifecycle

### Design Principles Validated

- **Reconciliation over events**: Backend events trigger bounded rescans; current graph is always authoritative
- **Idempotent operations**: Same observations always produce same graph state
- **Attachment generations**: Every reconnect increments generation to fence stale async operations
- **Identity confidence**: Strong identity (vendor/product/serial) enables durable DeviceId; weak identity (topology) creates ephemeral identity

## What's Next (Phase 1)

According to ADR-0031 implementation sequence:

1. **udev/sysfs adapter** - Read-only USB topology discovery
2. **UDisks2 adapter** - Storage functions and safe eject
3. **Composite device grouping** - USB-C docks appear as one device with multiple functions
4. **Structured blockers** - Safe removal reports specific blockers (open files, mounted volumes)
5. **Persistence** - Store device trust policy in `/var/lib/sol-deviced`

## Architecture Notes

### Backend Authority

Each domain keeps its authority:
- Compositor/libinput: input delivery
- Compositor/DRM: display topology
- sol-audiod/PipeWire: audio routing
- sol-networkd: network state
- BlueZ: Bluetooth pairing
- UDisks2: storage lifecycle
- UPower: battery/power

`sol-deviced` **coordinates** but does not **replace** these authorities.

### No Data Path

`sol-deviced` is a control plane only:
- No input events
- No audio/video samples
- No network packets
- No rendered frames

### Convergent Lifecycle

- Cold boot, daemon restart, suspend/resume all use same reconciliation path
- Events are invalidation hints, not commands
- Physical state is authoritative
- Bursts coalesce per device root

## Files Created

```
services/sol-deviced/
├── Cargo.toml
├── README.md
└── src/
    ├── main.rs              # Service entry point
    ├── types.rs             # Core device types
    ├── reconcile.rs         # Reconciliation engine
    ├── adapters/
    │   └── mod.rs          # Adapter trait + FakeAdapter
    └── dbus/
        └── mod.rs          # D-Bus interface
```

## Related ADRs

- [ADR-0031: Device control plane and hotplug](../docs/decisions/ADR-0031-device-control-plane-and-hotplug.md)
- [ADR-0011: Settings storage and API](../docs/decisions/ADR-0011-settings-storage-api.md)
- [ADR-0021: Application security and permissions](../docs/decisions/ADR-0021-application-security-permissions.md)
