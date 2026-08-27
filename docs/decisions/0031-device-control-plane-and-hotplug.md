# ADR-0031: Device control plane and hotplug lifecycle

- **Status:** Proposed
- **Date:** 2026-08-26
- **Target phase:** Phase 5 / daily-driver hardware integration
- **Extends:** ADR-0011 typed settings boundary and ADR-0021 application
  permissions

## Context

SOL needs one coherent representation of local and attached hardware for the
Shell, Settings, Files, system policy, diagnostics, and application permission
brokers. Linux exposes that hardware through several independent authorities:

- udev and sysfs expose the kernel device tree and hotplug notifications;
- the compositor and libinput own latency-sensitive input and display state;
- DRM/KMS exposes display topology;
- PipeWire and `sol-audiod` own audio routing and real-time audio endpoints;
- `sol-networkd` owns network interfaces and connection state;
- BlueZ owns Bluetooth discovery, pairing, and transport state;
- UDisks2 owns block-device mount, unmount, and power-off operations;
- UPower and protocol-specific services expose batteries and power state; and
- bolt, the IOMMU, and kernel policy participate in Thunderbolt/USB4 trust.

Presenting each source independently produces duplicate devices and incorrect
user-facing behavior. A USB-C dock may appear simultaneously as a hub, two
displays, an audio device, an Ethernet interface, a card reader, and a power
source. Kernel endpoint names such as `event7`, `card1`, `enp12s0`, and
`/dev/sdb` are connection-local implementation details rather than device
identities.

Hotplug is also not a sequence of reliable high-level add/remove commands.
One physical insertion produces a burst of partially ordered events while
drivers probe and child interfaces appear. Events can be duplicated, delayed,
or overtaken by removal. Suspend/resume, daemon restart, and backend restart
can lose notifications. A device may be physically removed while an earlier
authorization or probe operation is still completing.

SOL therefore needs a device boundary that provides stable identity,
composite-device grouping, convergent lifecycle state, authorization, safe
removal orchestration, and a quiet user experience without placing another
daemon in any latency-sensitive data path.

## Decision

SOL will provide a privileged system service named `sol-deviced` as the
**device control plane**. It owns the normalized device graph, identity and
attachment lifecycle, machine trust policy, cross-service operation
coordination, and the stable `org.sol.Device1` system API.

`sol-deviced` is not a driver manager and does not replace udev, libinput,
BlueZ, UDisks2, PipeWire, or subsystem services. It does not transport input
events, audio/video samples, network packets, or rendered frames.

### 1. Device graph

The public model has three levels:

```text
PhysicalDevice
└── Function
    └── Endpoint
```

- A **physical device** is the user-recognizable object, such as a dock,
  headset, keyboard, display, phone, camera, or storage enclosure.
- A **function** is one capability supplied by that device, such as audio
  output, microphone input, display output, network, input, storage, camera,
  or power delivery.
- An **endpoint** is a connection-local handle owned by a subsystem, such as a
  DRM connector, PipeWire node, network interface, block device, or libinput
  device.

A physical device may contain child physical devices. A hub or dock therefore
forms a graph rooted at the enclosure rather than a flat collection of udev
records. Functions record their authoritative owning service; they do not
transfer ownership to `sol-deviced`.

The normalized model separates independent state axes rather than encoding
every combination in one enum:

- **attachment:** discovering, present, quiescing, removed;
- **usability:** pending authorization, ready, degraded, blocked, unsupported;
- **activity:** idle or in use, with redacted owner information where allowed;
- **desired policy:** allowed, blocked, restricted, or managed;
- **health:** typed warnings and backend availability.

Immutable identity facts, discovered capabilities, mutable observed state,
desired policy, and current subsystem ownership remain distinct fields.
Unknown backend metadata is not copied into the stable public contract.

### 2. Identity and connection generations

The service uses different identifiers for different lifetimes:

- `DeviceId` is an opaque physical-device identity. It is durable only when
  the available evidence is strong enough to distinguish devices safely.
- `AttachmentId` identifies one connection instance. Every reconnect creates
  a new value even when the `DeviceId` is unchanged.
- `FunctionId` identifies a function under a physical device.
- `EndpointId` identifies a connection-local subsystem endpoint.
- `connection_generation` is a monotonic counter for a known device and fences
  all asynchronous probe, claim, authorization, and operation results.

Identity evidence has an explicit confidence level:

```text
cryptographic identity
    > vendor/product/serial identity
    > stable protocol identity
    > topology-derived identity
    > ephemeral attachment identity
```

Kernel names, enumeration order, or physical port path alone never produce a
durable `DeviceId`. Devices with topology-only or ephemeral identity do not
inherit an earlier device's permanent trust, user label, application grants,
or preferences merely because they occupy the same port.

Raw serial numbers, Bluetooth addresses, sysfs paths, and device nodes are
private backend data. Ordinary applications receive opaque identifiers and
scoped handles, not correlation identifiers.

### 3. Backend authority and ownership

Each field and operation has exactly one authoritative owner:

| Domain | Authoritative owner | `sol-deviced` responsibility |
|---|---|---|
| Kernel discovery and ancestry | udev/sysfs | Normalize and group physical devices |
| Input delivery | compositor/libinput | Inventory, trust, settings link, removal state |
| Display topology and modes | compositor/DRM | Physical identity, grouping, remembered-device link |
| Audio routing | `sol-audiod`/PipeWire | Attach audio functions and summarize route/activity |
| Network state | `sol-networkd` | Attach network functions and coordinate device actions |
| Bluetooth transport and pairing | BlueZ | Present one device, apply SOL trust policy, coordinate forget/disconnect |
| Storage lifecycle | UDisks2 | Aggregate volumes and coordinate safe removal |
| Battery and power | UPower/protocol backend | Normalize charge, charging, and power capability |
| App access | `sol-securityd` and portals | Supply function identity and enforce attachment fencing |

An adapter reports observations and accepts only operations belonging to its
domain. `sol-deviced` never guesses a domain state from a lower-level event
when the authoritative domain service is available. Backend loss marks the
affected function degraded; it does not erase the whole physical device.

### 4. Reconciliation instead of event replay

Backend events are invalidation hints that trigger a bounded rescan. They are
not directly published as device lifecycle events.

For every backend, `sol-deviced` will:

1. establish monitoring before or atomically with initial enumeration so that
   coldplug cannot leave an enumerate/subscribe gap;
2. normalize authoritative observations into a candidate device graph;
3. reconcile that graph with the current graph using idempotent add, update,
   claim, and removal transitions;
4. reject asynchronous results whose `AttachmentId` or
   `connection_generation` is stale; and
5. publish one complete, monotonically revisioned snapshot after a coherent
   graph transition.

Bursts may be coalesced per physical root for control-plane stability. Such
coalescing must never delay compositor input dispatch, display presentation,
audio processing, or another subsystem's own hotplug response.

Cold boot, daemon restart, backend restart, resume, and explicit recovery all
use the same reconciliation path. A full reconciliation is mandatory after
resume and whenever an adapter reports an event gap or loses its connection to
the underlying service.

Removal is idempotent. Physical disappearance immediately fences new access,
invalidates the attachment generation, and begins lease revocation and owner
cleanup. Cleanup may finish after the graph reports the device absent, but a
late cleanup or probe result can never revive the removed attachment.

### 5. Operations and safe removal

All public mutations are asynchronous operations with a stable `OperationId`
and typed state:

```text
pending -> running -> waiting-for-user -> completed
                   \-> failed
                   \-> cancelled
```

Operations are idempotent when retried with the same request identifier. They
carry deadlines, attachment generations, progress, typed failure reasons, and
redacted blockers suitable for trusted UI.

Safe removal is a coordinated, best-effort transaction rather than a synonym
for disappearance:

1. resolve the affected physical device and functions;
2. enter `quiescing` and reject new claims or leases;
3. ask authoritative participants to prepare and report blockers;
4. flush, unmount, disconnect, release, or power off the functions for which
   those actions are required and supported;
5. revoke remaining attachment-scoped leases; and
6. report completion only after every required participant acknowledges safe
   removal.

Prepare, commit, abort, and recovery hooks are idempotent. If preparation
fails, the operation reports structured blockers and restores usable desired
state where the device remains present. Physical removal during preparation
switches to the unexpected-removal path; it is never reported as safely
removed.

An operation may cover one function, such as ejecting a storage volume, or the
whole physical device, such as disconnecting a composite dock. The API exposes
only actions supported by the current device graph.

### 6. Trust, permissions, and leases

Machine policy distinguishes unknown, allowed, restricted, blocked, and
managed devices. Transport-specific enforcement, including Thunderbolt/USB4
authorization and locked-session input policy, is implemented by the relevant
kernel or subsystem authority and coordinated by `sol-deviced`.

Applications are never granted a physical device or unrestricted `/dev`
access. An application requests a typed function capability through the
appropriate SOL portal or broker, for example camera capture, microphone
input, or access to a user-selected volume. The resulting lease is bound to:

- authenticated `AppId` and publisher lineage;
- `DeviceId` and `FunctionId`;
- `AttachmentId` and `connection_generation`;
- allowed operations and data scope;
- grant generation, expiry, and revocation state.

`sol-securityd` remains the permission authority and durable audit ledger.
`sol-deviced` supplies current function identity and attachment state and
participates in grant/revocation transactions. Device removal, trust-policy
change, app exit, grant revocation, or generation mismatch makes a lease
unusable immediately.

Calls on `org.sol.Device1` are authenticated and authorized by operation.
Untrusted applications do not gain authority by reaching the system-bus name;
their supported surface is the typed SolKit API and portals. Privileged
`device_admin` authority is restricted to trusted system components and
explicit managed policy.

### 7. Stable service API

`sol-deviced` owns the system-bus name `org.sol.Device1` and root path
`/org/sol/Device1`. The stable contract is typed and versioned. Its primary
read interface is a complete `DeviceSnapshot` containing a monotonic revision,
physical-device graph, supported actions, outstanding attention items, and
operation summaries.

The initial contract includes semantic operations equivalent to:

```text
GetSnapshot()
GetDevice(DeviceId)
Authorize(DeviceId, AuthorizationScope, RequestId)
SetTrust(DeviceId, TrustPolicy, RequestId)
Forget(DeviceId, RequestId)
Eject(FunctionId, RequestId)
PrepareRemoval(DeviceId, RequestId)
CancelOperation(OperationId)
GetOperation(OperationId)
```

Signals announce `SnapshotChanged(revision)`, `OperationChanged(operation_id)`,
and `AttentionRequired(attention_id)`. Signals are wakeups rather than the sole
record of truth: after a missed signal or reconnect, a client recovers by
reading a complete snapshot. The wire contract uses named typed structures and
enums; unbounded `a{sv}` maps, sysfs dictionaries, raw backend errors, paths,
and device nodes are not public API.

Settings provides user-facing mutations through its typed API and delegates
to `sol-deviced`; it does not implement device policy itself. Files invokes
storage operations through the typed device/storage boundary. Shell renders
trusted notifications and attention UI but owns no device state.

### 8. Persistence and privacy

Machine-scoped identity evidence, trust policy, managed policy, and schema
version are persisted atomically under `/var/lib/sol-deviced`. Volatile
attachments, endpoints, operations, activity, and leases live only in memory
or under `/run/sol-deviced` and are reconstructed by reconciliation.

User labels and user-specific preferences are account-scoped records reached
through the settings/accounts boundary. They are not silently converted into
machine-wide trust. Only devices with sufficient identity confidence retain
such associations across reconnects.

Disconnected history is retained only when it has durable value: the device
was paired, explicitly trusted or blocked, user-named, managed, or has saved
preferences. Incidental ephemeral devices age out. Diagnostics and logs use
opaque IDs, typed reason codes, bounded retention, and deterministic redaction;
they do not record raw serial numbers, Bluetooth addresses, mount contents, or
application data.

### 9. User-experience contract

The trusted UI presents one card per user-recognizable physical device and
nests its functions. Normal arrival of already allowed keyboards, mice, and
other routine devices is silent. UI is shown only when a decision, failure,
privacy change, route change, or useful action exists.

Required behaviors include:

- a composite dock appears once rather than as unrelated endpoints;
- remembered configuration is applied only to a confidently identified
  device;
- unknown or restricted hardware offers typed choices such as allow once,
  always allow, or block when policy permits them;
- storage arrival offers actions such as open and eject;
- safe-removal progress and blockers are visible and actionable;
- unexpected storage removal is distinguished from successful eject;
- transient backend loss appears as degraded state rather than false physical
  removal; and
- disconnected history contains saved devices, not every endpoint ever seen.

## Implementation sequence

The first vertical slice will be deliberately narrow:

1. Define transport-independent device, function, endpoint, identity,
   operation, and snapshot types.
2. Implement a deterministic reconciler and fake adapters before privileged
   hardware integration.
3. Add read-only udev/sysfs discovery for USB topology and block devices.
4. Implement composite-device grouping, identity confidence, attachment
   generations, restart recovery, and revisioned snapshots.
5. Integrate UDisks2 and complete storage eject with structured blockers.
6. Expose `org.sol.Device1` and add typed proxies for trusted system clients.
7. Integrate Settings and Files.
8. Add BlueZ, audio, network, display/input, power, and security lease adapters
   without changing the core graph or lifecycle contract.

The crate will keep transport adapters behind traits so that lifecycle and
policy tests do not require real hardware or a live system bus.

## Consequences

### Positive

- SOL gains one coherent inventory and user-facing representation of composite
  devices without centralizing their data paths.
- Hotplug converges after missed, duplicate, delayed, and out-of-order events.
- Restarts and suspend/resume use the same recovery model as initial startup.
- Stable identity can safely support remembered preferences and trust without
  relying on kernel endpoint names.
- Application access is scoped to functions and fenced to a particular
  attachment generation.
- Safe removal can explain blockers and coordinate all relevant services.

### Costs and risks

- Correct physical-device grouping requires transport-specific heuristics and
  explicit identity confidence.
- Cross-service operations require idempotency, timeouts, cancellation, and
  recovery behavior in every participant.
- Complete typed snapshots require schema evolution and compatibility tests.
- Some hardware supplies no reliable durable identity, so SOL must accept an
  intentionally ephemeral experience instead of remembering unsafe policy.
- During incremental rollout, legacy subsystem discovery and `sol-deviced`
  observations may coexist; authoritative ownership must remain explicit to
  prevent feedback loops.

## Alternatives considered

### Expose udev events directly

Rejected. Udev records are backend details, composite devices generate event
bursts, and event replay cannot recover reliably from daemon restart or missed
notifications.

### Make `sol-deviced` own all hardware operations and data paths

Rejected. This duplicates mature subsystem authorities, expands privilege,
creates a single failure domain, and risks input, display, audio, and network
latency.

### Let every subsystem expose an independent device list to Settings

Rejected. It cannot produce stable physical-device grouping, consistent trust,
cross-function safe removal, or a coherent notification policy.

### Persist identities from device nodes or physical port paths

Rejected. Enumeration names change, and port-based identity can transfer trust
or grants to a different physical device.

### Treat hotplug as an append-only event stream

Rejected. Physical state is authoritative; the service must reconcile current
observations and use events only to trigger that reconciliation.

## Required tests

- Coldplug and the equivalent hotplug sequence produce identical snapshots.
- Subscribe/enumerate races do not omit a device or publish it twice.
- Duplicate and reordered backend events converge to the same graph.
- A USB-C dock with display, audio, network, and storage endpoints appears as
  one physical device with multiple functions.
- Endpoint renumbering across reconnect does not change a confidently derived
  `DeviceId`.
- A topology-only device does not inherit durable trust after replacement on
  the same port.
- A delayed probe or authorization result from generation N cannot affect
  generation N+1.
- Physical removal immediately prevents new leases even when cleanup is still
  running.
- Daemon restart and suspend/resume reconstruct the graph without phantom
  devices or duplicate notifications.
- Loss of one domain backend degrades only its functions and recovers by
  reconciliation.
- Safe eject reports structured blockers and never reports completion before
  all required participants acknowledge it.
- Physical removal during safe eject is reported as unexpected removal.
- Retrying an operation with the same request ID is idempotent.
- An application cannot use a function lease from another App ID, device,
  attachment, generation, or authorization generation.
- Public snapshots and diagnostics do not expose raw serial numbers, device
  nodes, sysfs paths, Bluetooth addresses, or unbounded backend metadata.
- Fuzzed device graphs cannot create cycles, orphan functions, duplicate
  endpoint ownership, or unbounded retained history.

## Non-claims

This ADR does not select the persistent database encoding, final D-Bus wire
encoding, USB authorization defaults, Bluetooth pairing UI, display-layout
policy, filesystem formats, driver installation mechanism, firmware update
service, or exact hardware-support matrix. Those choices must preserve the
identity, authority, reconciliation, lease, and privacy boundaries defined
here.

## Related

- [SOL Architecture](../architecture.md)
- [SOL Product Requirements](../PRD.md)
- [ADR-0011: Settings storage and API](0011-settings-storage-api.md)
- [ADR-0021: Application security and permissions](0021-application-security-permissions.md)
- [Compositor development path](0005-compositor-dev-path.md)
