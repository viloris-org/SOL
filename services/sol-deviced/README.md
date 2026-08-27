# sol-deviced

Device control plane service for SOL OS.

Implements [ADR-0031: Device control plane and hotplug lifecycle](../../docs/decisions/ADR-0031-device-control-plane-and-hotplug.md).

## Purpose

`sol-deviced` provides:
- Stable device identity and composite device grouping
- Convergent hotplug lifecycle (handles bursts, duplicates, restarts)
- Device authorization and trust policy
- Safe removal orchestration across subsystems
- Stable `org.sol.Device1` D-Bus API

## Architecture

Three-level device graph:
```
PhysicalDevice (user-recognizable object)
└── Function (capability: audio, display, storage, network, etc.)
    └── Endpoint (connection-local subsystem handle)
```

Key principles:
- **Reconciliation over events**: Backend events trigger bounded rescans; the current graph is always authoritative
- **Backend ownership**: Each domain (audio, network, storage) keeps its authority; `sol-deviced` coordinates
- **Attachment generations**: Every reconnect increments generation to fence stale operations
- **Identity confidence**: Crypto > vendor/product/serial > stable protocol > topology > ephemeral

## D-Bus API

**Bus name**: `org.sol.Device1`  
**Object path**: `/org/sol/Device1`

### Methods

- `GetSnapshot() -> (revision: u64, json: String)` - Complete device snapshot
- `GetDevice(device_id: String) -> json: String` - Single device details
- `Authorize(device_id: String, scope: String, request_id: String) -> request_id: String`
- `SetTrust(device_id: String, policy: String, request_id: String) -> request_id: String`
- `Forget(device_id: String, request_id: String) -> request_id: String`
- `Eject(function_id: String, request_id: String) -> request_id: String`
- `PrepareRemoval(device_id: String, request_id: String) -> request_id: String`
- `CancelOperation(operation_id: String)`
- `GetOperation(operation_id: String) -> json: String`

### Signals

- `SnapshotChanged(revision: u64)` - Snapshot updated
- `OperationChanged(operation_id: String)` - Operation state changed
- `AttentionRequired(attention_id: String)` - User action needed

## Implementation Status

Phase 0 (current):
- ✅ Core types (DeviceId, Function, Endpoint, identity confidence)
- ✅ Reconciler with idempotent add/update/remove
- ✅ Fake adapter for testing
- ✅ D-Bus interface skeleton
- ✅ Basic service loop

Phase 1 (next):
- [ ] udev/sysfs adapter for USB topology
- [ ] UDisks2 adapter for storage functions
- [ ] Composite device grouping (USB-C docks)
- [ ] Safe eject with blockers
- [ ] Persistence (/var/lib/sol-deviced)

Phase 2+:
- [ ] BlueZ adapter
- [ ] Audio adapter (PipeWire)
- [ ] Network adapter (sol-networkd)
- [ ] Display/input adapter (compositor)
- [ ] Security lease integration

## Running

```bash
# Build
cargo build -p sol-deviced

# Run (requires system bus)
cargo run -p sol-deviced

# Run tests
cargo test -p sol-deviced
```

## Testing

The reconciler includes deterministic tests:
- Coldplug equals hotplug sequence
- Duplicate events converge
- Removal fences attachment generation
- Backend restart recovery

Use `FakeAdapter` to test without real hardware.

## Related

- [ADR-0031: Device control plane](../../docs/decisions/ADR-0031-device-control-plane-and-hotplug.md)
- [ADR-0011: Settings storage and API](../../docs/decisions/ADR-0011-settings-storage-api.md)
- [ADR-0021: Application security and permissions](../../docs/decisions/ADR-0021-application-security-permissions.md)
