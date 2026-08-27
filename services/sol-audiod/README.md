# sol-audiod

SOL's unified audio service daemon - handles device routing, Bluetooth audio, and low-latency audio management.

## Features

- **Intelligent device routing** - Headphones automatically take priority over speakers
- **Bluetooth integration** - Automatic device classification and seamless switching
- **Context-aware switching** - Respects calls, screen mirroring, and multi-user scenarios
- **Low-latency path** - Real-time scheduling for audio threads (integrates with ADR-0029)
- **Battery awareness** - Adjusts routing based on device battery levels
- **User preferences** - Trusted devices, priority boosts, per-device auto-switch settings

## Architecture

```
Applications
     ↓ sol-audio SDK (capability-gated)
sol-audiod (this service)
     ↓ D-Bus IPC + shared memory
PipeWire/ALSA
     ↓
Hardware (ALSA kernel drivers)
```

## Configuration

Configuration file: `~/.config/sol/audiod.toml`

```toml
[routing]
auto_switch_headphones = true
auto_switch_speakers = false
auto_switch_wired = true
crossfade_duration_ms = 300
detect_shared_usage = true
battery_aware = true

[routing.priority_boosts]
"00:1A:7D:DA:71:13" = 20  # Custom priority boost for specific device

[bluetooth]
prefer_codec = "ldac"
auto_reconnect_last_device = true
connection_timeout_sec = 5

# Per-device configuration
[devices."00:1A:7D:DA:71:13"]
name = "Sony WH-1000XM5"
type = "headphones"
auto_switch = true
trusted = true
classification_source = "vendor_db"
```

## Device Classification

Devices are classified using multiple signal sources (in priority order):

1. **User manual override** - Explicit user classification
2. **Vendor database** - Known vendor/product ID pairs
3. **Bluetooth CoD** - Class of Device field
4. **Name patterns** - Device name matching
5. **Audio profiles** - HFP/HSP/A2DP UUID analysis

## Routing Priority

Base priorities (higher = more preferred):

- Wired headphones: 100
- Wired speakers: 95
- Bluetooth earbuds: 80
- Bluetooth headphones: 75
- Car audio: 60
- Speakers: 40
- Soundbar: 35
- HDMI: 20
- Built-in speaker: 10

Dynamic modifiers:

- User explicit choice: +50
- Recently used: +10
- Trusted device: +5
- Low battery (<15%): -20
- In call: +30
- Screen mirroring: -15
- Multiple users: -25

## Building

```bash
cargo build -p sol-audiod
```

## Running

```bash
cargo run -p sol-audiod
```

## Testing

```bash
cargo test -p sol-audiod
scripts/validate-audiod-dbus.sh
```

## Integration

### Implemented
- Bluetooth device discovery and classification
- Priority-based routing logic
- Configuration system
- `org.sol.Audio1` D-Bus control plane
- PipeWire output discovery through structured `pactl` JSON
- Real default-output switching, including migration of existing sink inputs
- Hotplug reconciliation, automatic headphone switching, and disconnect fallback
- D-Bus signals for device connection, disconnection, and active-route changes

### Next
- Native PipeWire registry/event API (replace the current Pulse compatibility adapter)
- Crossfade implementation
- Persist preference changes made through D-Bus

### Phase 3 (Future)
- Zero-copy shared memory audio transport
- Real-time scheduling (SCHED_FIFO)
- Context detection (calls, screen mirroring)
- Battery monitoring
- Audio plugin framework

## D-Bus Interface

```
Service: org.sol.Audio1
Object: /org/sol/Audio1

Methods:
  - ListDevices() -> Vec<DeviceInfo>
  - GetActiveDevice() -> DeviceInfo
  - SetOutputDevice(device_id: String)
  - SetDevicePreference(device_id: String, auto_switch: bool)
  - SetDeviceTrusted(device_id: String, trusted: bool)
  - RefreshDevices() -> Vec<DeviceInfo>

Signals:
  - DeviceChanged(old: String, new: String)
  - DeviceConnected(device_id: String)
  - DeviceDisconnected(device_id: String)
```

`DeviceInfo` is transferred as `(id, name, type, connected, battery, trusted,
priority)`. Battery is `-1` when the backend does not expose it.

At runtime `pactl` must resolve to PipeWire's Pulse compatibility service. The
daemon rejects a legacy PulseAudio server rather than silently controlling an
unsupported backend.

## See Also

- [ADR-0029](../../docs/decisions/ADR-0029-process-scheduling.md) - Process scheduling strategy (RT priorities)
- [Architecture](../../docs/architecture.md) - Overall SOL architecture
