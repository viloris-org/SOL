# sol-networkd

Network management service for SOL OS. Handles device discovery, connection profiles, DHCP, DNS configuration, and connectivity monitoring.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    org.sol.Network1                          │
│                      (D-Bus API)                             │
├─────────────────────────────────────────────────────────────┤
│                   NetworkManager                             │
│  ┌────────────┬──────────────┬───────────────────────────┐  │
│  │  Devices   │   Profiles   │  Connectivity Monitor     │  │
│  └────────────┴──────────────┴───────────────────────────┘  │
├─────────────────────────────────────────────────────────────┤
│  NetlinkMonitor │ DhcpClient │ DnsManager │ SecretStore    │
└─────────────────────────────────────────────────────────────┘
         │               │            │
    rtnetlink       dhcproto    systemd-resolved
```

## Phase 1 Implementation Status

### ✅ Completed

**Core Infrastructure**
- NetworkManager with device/profile/connectivity state management
- D-Bus service on `org.sol.Network1` with Manager/Device/Profile/WiFi/VPN interfaces
- Netlink integration via rtnetlink for device monitoring
- Device abstraction (WiFi/Ethernet/VPN types with per-type handlers)

**Device Management**
- Interface discovery via netlink + /sys/class/net
- Device state tracking (Unavailable → Active lifecycle)
- Per-device-type specialization (WiFiDevice, EthernetDevice, VpnDevice)

**WiFi Support**
- D-Bus WiFi interface (`org.sol.Network1.WiFi`)
- WiFi device abstraction with iwd backend integration structure
- Network scanning API (Scan, GetNetworks)
- Connection/disconnection methods
- Signal strength and current network properties
- Enable/disable WiFi radio control
- Network information dictionary (SSID, BSSID, signal, frequency, security)

**VPN Support**
- D-Bus VPN interface (`org.sol.Network1.VPN`)
- WireGuard configuration structures with multi-peer support
- VPN device abstraction
- Profile management (create, list, delete)
- Connection/disconnection methods
- VPN status reporting (connection state, traffic statistics)
- Support structures for OpenVPN and IPSec (implementation pending)

**Profile System**
- Connection profile storage (WiFi/Ethernet/VPN)
- Profile-to-device activation flow
- Auto-connect policy framework
- WiFi profiles with security type and passphrase storage
- VPN profiles (WireGuard/OpenVPN/IPSec configurations)
- Profile persistence (structure ready, file I/O pending Phase 2)

**DHCP Client**
- RFC 2131 DHCPv4 implementation via dhcproto
- DISCOVER → OFFER → REQUEST → ACK state machine
- Lease acquisition, renewal, and release
- Integration with NetworkManager for automatic IP configuration

**DNS Integration**
- systemd-resolved D-Bus client
- Per-interface DNS server configuration via SetLinkDNS
- Search domain configuration via SetLinkDomains
- Cache flushing (FlushCaches)

**Connectivity Monitoring**
- RFC 8952 captive portal detection
- Multi-endpoint validation (connectivity-check.ubuntu.com, detectportal.firefox.com, clients3.google.com)
- Portal state machine (Unknown → Portal/Limited/Full)
- Integration with NetworkManager for automatic connectivity checks

**Security**
- SecretStore using ring for credential encryption (AES-256-GCM)
- Separate encryption per profile (WiFi passphrase, VPN credentials)
- Secure VPN private key storage
- Credential handling without logging

### 🚧 Phase 2 Pending

**WiFi Backend Integration**
- Complete iwd D-Bus integration for real scanning
- Real-time signal strength monitoring
- Network quality metrics collection
- Background scanning and roaming
- WPS (WiFi Protected Setup)
- Hidden network support

**VPN Backend Integration**
- WireGuard kernel module integration via wireguard-control
- Interface creation and IP configuration
- Routing table management
- Split tunneling support
- OpenVPN backend implementation
- IPSec/IKEv2 backend (strongSwan integration)
- Automatic reconnection on network changes

**Profile Persistence**
- Profile file I/O (JSON or TOML)
- Profile validation on load
- Migration on schema changes

**Advanced Features**
- IPv6 support (DHCPv6, SLAAC)
- Connection prioritization (prefer ethernet, avoid metered)
- Time/location-based auto-connect policies
- Hotspot mode (AP + DHCP server)
- 802.1X enterprise WiFi authentication
- Traffic shaping and QoS

## D-Bus API

### org.sol.Network1.Manager

**Methods:**
- `State() -> s` - Returns current network state string
- `Connectivity() -> u` - Returns connectivity level (0=none, 1=portal, 2=limited, 3=full)
- `ListDevices() -> ao` - Returns device object paths
- `ConnectToProfile(s profile_id) -> o` - Returns active connection path
- `DisconnectProfile(s profile_id) -> ()`
- `ListProfiles() -> ao` - Returns profile object paths

**Signals:**
- `StateChanged(s new_state)` - Network state transition
- `ConnectivityChanged(u new_level)` - Connectivity level changed
- `DeviceAdded(o device)` - New device detected
- `DeviceRemoved(o device)` - Device removed

### org.sol.Network1.WiFi

**Methods:**
- `Scan() -> ()` - Trigger WiFi network scan
- `GetNetworks() -> aa{sv}` - Get available networks (SSID, BSSID, signal, security)
- `Connect(ss ssid, ss passphrase) -> ()` - Connect to WiFi network
- `Disconnect() -> ()` - Disconnect from current network

**Properties:**
- `SignalStrength: y` (readable) - Current signal strength (0-100)
- `CurrentNetwork: s` (readable) - Currently connected SSID
- `Enabled: b` (readwrite) - WiFi radio enabled state

**Signals:**
- `ScanComplete()` - Scan finished
- `NetworkAdded(a{sv})` - New network detected

### org.sol.Network1.VPN

**Methods:**
- `Connect(s profile_id) -> ()` - Connect to VPN
- `Disconnect(s profile_id) -> ()` - Disconnect from VPN
- `CreateWireGuardProfile(a{sv} config) -> s` - Create WireGuard profile, returns profile_id
- `ListProfiles() -> as` - List VPN profile IDs
- `GetStatus(s profile_id) -> a{sv}` - Get connection status (connected, bytes_sent, bytes_received)
- `DeleteProfile(s profile_id) -> ()` - Delete VPN profile

**Signals:**
- `ConnectionStateChanged(s profile_id, b connected)` - VPN connection state changed

### org.sol.Network1.Device

**Properties:**
- `DeviceType: s` - "wifi", "ethernet", "vpn"
- `State: s` - Device state string
- `Interface: s` - Linux interface name (wlan0, eth0, etc.)

**Methods (WiFi-specific):**
- `Scan() -> ()`
- `GetNetworks() -> aa{sv}` - Returns SSID/signal/security dicts

### org.sol.Network1.Profile

**Properties:**
- `Id: s` - Profile UUID
- `Name: s` - User-visible name
- `ProfileType: s` - "wifi", "ethernet", "vpn"
- `AutoConnect: b` (readwrite) - Auto-connect on availability
- `Metered: b` (readwrite) - Treat as metered connection

**Methods:**
- `Connect() -> o` - Activate and return connection path
- `Disconnect() -> ()`
- `Delete() -> ()`

For complete API documentation, see [docs/dbus-api.md](docs/dbus-api.md).

## Building

```bash
cargo build -p sol-networkd
```

## Running

Requires root for netlink and DHCP socket operations:

```bash
sudo cargo run -p sol-networkd
```

The service will claim `org.sol.Network1` on the session bus and begin monitoring network devices.

## Testing

```bash
cargo test -p sol-networkd
```

Integration tests require D-Bus session bus and rtnetlink permissions.

## Dependencies

- **rtnetlink** - Netlink interface for device enumeration
- **dhcproto** - DHCPv4 protocol implementation
- **zbus** - D-Bus interface and service hosting
- **ring** - Cryptographic primitives for secret storage
- **wireguard-control** - WireGuard VPN configuration
- **reqwest** - HTTP client for connectivity checking
- **tokio** - Async runtime
- **anyhow** - Error handling
- **thiserror** - Error type derivation

## External Dependencies

- **iwd** (Intel's iNet Wireless Daemon) - WiFi backend (Phase 2)
- **systemd-resolved** - DNS resolution
- **WireGuard kernel module** - VPN tunneling (Phase 2)

## Architecture Notes

**Device State Lifecycle:**
```
Unavailable → Disconnected → Preparing → Configuring → NeedAuth
                                    ↓
                             IpConfig → IpCheck → Active
                                              ↓
                                         Deactivating → Failed
```

**Connection Flow:**
1. User calls `ActivateConnection(profile_id, device)`
2. Manager validates profile and device compatibility
3. Device transitions to Preparing state
4. DHCP client acquires lease (Configuring → IpConfig)
5. DNS manager configures resolvers
6. Connectivity check runs (IpCheck → Active)
7. Manager emits StateChanged signal

**Profile Storage:**
Profiles are stored in-memory (HashMap) in Phase 1. Phase 2 will persist to `~/.config/sol/network/profiles/` as JSON.

**Security Model:**
- WiFi passphrases encrypted at rest via SecretStore
- VPN credentials stored separately from connection metadata
- DHCP transactions use transaction IDs to prevent spoofing
- DNS queries go through systemd-resolved for DNSSEC support

## Future Work (Phase 2+)

- [ ] Complete iwd D-Bus integration for WiFi scanning and authentication
- [ ] WireGuard kernel module integration via wireguard-control
- [ ] OpenVPN and IPSec backend implementations
- [ ] Profile file persistence
- [ ] IPv6 DHCPv6 and SLAAC
- [ ] Hotspot mode (AP + DHCP server)
- [ ] Connection statistics and usage tracking
- [ ] Network cost awareness (prefer WiFi over cellular)
- [ ] Captive portal web view integration
- [ ] MAC address randomization
- [ ] 802.1X enterprise WiFi authentication
- [ ] mDNS/DNS-SD service discovery
- [ ] Network Time Security (NTS) client

## Documentation

- [D-Bus API Reference](docs/dbus-api.md) - Complete API specification
- [WiFi and VPN Usage Guide](docs/wifi-vpn-usage.md) - Usage examples with busctl and Python
- [Implementation Status](docs/implementation-status.md) - Detailed phase-by-phase status
- [Architecture Overview](docs/architecture.md) - System design and component interaction (coming soon)
