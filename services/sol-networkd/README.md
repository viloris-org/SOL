# sol-networkd

Network management service for SOL OS. Handles device discovery, connection profiles, DHCP, DNS configuration, and connectivity monitoring.

## Reliability update (2026-08)

The service now has a tested reliability baseline for its core control path:

- subscribes to real kernel link, address, and route multicast events instead of producing timer-based placeholder events;
- performs guarded auto-connect selection, preferring Ethernet and optionally excluding metered profiles;
- rolls failed connection attempts back to `Disconnected` and tracks the active profile and observed internet connectivity separately;
- implements the global WiFi D-Bus scan cache, connect/disconnect, signal, current-network, and radio-power operations;
- validates D-Bus object-path components instead of silently dropping IDs containing `:` or `-`;
- accepts only the expected DHCP OFFER/ACK for the current transaction and exits the receive loop correctly;
- applies addresses and routes with checked `ip ... replace` operations and configures DNS through `systemd-resolved` rather than overwriting `/etc/resolv.conf`; and
- stores profiles atomically with private permissions, rejects unsafe/duplicate IDs, and fails rather than silently losing a newly generated credential salt.

Run the focused test suite with `cargo test -p sol-networkd --all-targets`.

## systemd-networkd inspired improvements (2026-08)

Inspired by systemd-networkd's robust architecture, the following enhancements have been implemented:

- **Extended netlink event handling** - Now monitors link state changes, neighbor updates, routing rules, and detailed route/address events with full attribute extraction
- **Enhanced device state tracking** - Devices now track ifindex, carrier status, MAC address, MTU, and IP address lists
- **State file persistence** - Runtime state saved to `/run/sol-networkd/state` (similar to systemd's `/run/systemd/netif/state`) with operational/carrier/address/online state aggregation
- **Configuration request queue** - Serialized configuration operations prevent race conditions during concurrent network changes
- **Comprehensive event handlers** - Separate handlers for link up/down/change, address add/remove, and route changes with automatic state persistence

### Known implementation boundaries

- iwd integration still needs validation and hardening against a real iwd daemon, including hidden and enterprise networks.
- Manager-level VPN activation is explicitly rejected until profile secrets, interface creation, routes, and rollback are wired end to end. The lower-level WireGuard structures are not yet a complete connection flow.
- DHCP renewal/release primitives exist, but lease scheduling and persistence are not implemented.
- `NtsClient` currently delegates to system time services; it is not an in-process RFC 8915 NTS implementation.
- D-Bus signals for device and profile state changes are not yet emitted (polling sync is used instead).
- Immediate device/profile object registration on change is not implemented (currently polls every 5 seconds).

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

### ✅ Completed (Latest Updates)

**D-Bus Infrastructure**
- D-Bus service on `org.sol.Network1` with Manager/Device/Profile/WiFi/VPN interfaces
- **Dynamic device object registration** - Devices automatically get D-Bus objects at `/org/sol/Network1/Device/{id}`
- **Dynamic profile object registration** - Profiles automatically get D-Bus objects at `/org/sol/Network1/Profile/{id}`
- **Background synchronization** - Periodic sync ensures D-Bus objects match NetworkManager state
- **Profile lifecycle methods** - Connect/Disconnect/Delete operations fully wired to NetworkManager

**Core Infrastructure**
- NetworkManager with device/profile/connectivity state management
- Netlink integration via rtnetlink for device monitoring
- Device abstraction (WiFi/Ethernet/VPN types with per-type handlers)
- **Profile persistence** - Profiles saved to `/var/lib/sol-networkd/profiles/*.json`
- **Secure credential storage** - PBKDF2-derived keys with AES-256-GCM encryption
- **Network Time Security (NTS)** - Automatic time sync after connection
- **Enhanced connection flow** - Complete DHCP → IP config → DNS → time sync pipeline

**Device Management**
- Interface discovery via netlink + /sys/class/net
- Device state tracking (Unavailable → Active lifecycle)
- Per-device-type specialization (WiFiDevice, EthernetDevice, VpnDevice)
- **Ethernet device implementation** - DHCP, static IP, carrier detection, link speed
- **Automatic IP configuration** - Apply DHCP leases to network interfaces
- **Interface management** - Link up/down, address assignment, routing

**WiFi Support**
- D-Bus WiFi interface (`org.sol.Network1.WiFi`)
- WiFi device abstraction with iwd backend integration structure
- Network scanning API (Scan, GetNetworks)
- Connection/disconnection methods
- Signal strength and current network properties
- Enable/disable WiFi radio control
- Network information dictionary (SSID, BSSID, signal, frequency, security)
- **Quick connect API** - Single-call WiFi connection with auto-profile creation
- **WPA2/Open security** - Automatic security type detection

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
- **Profile persistence** - JSON serialization to `/var/lib/sol-networkd/profiles/`
- **Automatic loading** - Profiles restored on daemon startup
- **Profile CRUD** - Create, read, update, delete with atomic file operations

**DHCP Client**
- RFC 2131 DHCPv4 implementation via dhcproto
- DISCOVER → OFFER → REQUEST → ACK state machine
- Lease acquisition, renewal, and release
- Integration with NetworkManager for automatic IP configuration
- **Lease renewal** - Background task for lease maintenance
- **Multiple DNS servers** - Support for primary and secondary DNS
- **Gateway and netmask** - Full IP configuration from DHCP response

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
- **PBKDF2 key derivation** - Machine-specific keys from hardware ID
- **Per-profile nonces** - Unique encryption context for each credential
- **Automatic key rotation** - Re-derive keys on machine ID changes

**Network Time Security (NTS)**
- RFC 8915 NTS-KE client implementation
- Automatic time synchronization after network connection
- TLS 1.3 connection to time.cloudflare.com
- Cookie-based authenticated NTP requests
- Integration with NetworkManager connection flow
- Time drift monitoring and correction

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
- `ScanWifi() -> aa{sv}` - Scan and return available WiFi networks
- `ConnectWifi(ss ssid, ss passphrase) -> s` - Quick connect to WiFi, returns profile_id
- `SyncTime() -> a{sv}` - Trigger NTS time sync, returns time info

**Properties:**
- `TimeInfo: a{sv}` - Current time and synchronization status

**Signals:**
- `StateChanged(s new_state)` - Network state transition
- `ConnectivityChanged(u new_level)` - Connectivity level changed
- `DeviceAdded(o device)` - New device detected
- `DeviceRemoved(o device)` - Device removed
- `TimeSynchronized(t timestamp)` - System time updated via NTS

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
Profiles are stored in `/var/lib/sol-networkd/profiles/` as JSON files, one per profile. Each file is named by its UUID and contains encrypted credentials. The SecretStore derives encryption keys from the machine's hardware ID using PBKDF2-HMAC-SHA256 with 100,000 iterations.

Example profile file structure:
```json
{
  "WiFi": {
    "ssid": "HomeNetwork",
    "security": "Wpa2",
    "passphrase": "base64_encrypted_data_here",
    "auto_connect": true,
    "metered": false
  }
}
```

**Security Model:**
- WiFi passphrases encrypted at rest via SecretStore
- VPN credentials stored separately from connection metadata
- DHCP transactions use transaction IDs to prevent spoofing
- DNS queries go through systemd-resolved for DNSSEC support

## Future Work (Phase 2+)

- [ ] Complete iwd D-Bus integration for WiFi scanning and authentication
- [ ] WireGuard kernel module integration via wireguard-control
- [ ] OpenVPN and IPSec backend implementations
- [ ] IPv6 DHCPv6 and SLAAC
- [ ] Hotspot mode (AP + DHCP server)
- [ ] Connection statistics and usage tracking
- [ ] Network cost awareness (prefer WiFi over cellular)
- [ ] Captive portal web view integration
- [ ] MAC address randomization
- [ ] 802.1X enterprise WiFi authentication
- [ ] mDNS/DNS-SD service discovery
- [ ] NTP fallback when NTS is unavailable
- [ ] Connection migration (seamless switch between networks)

## Documentation

- [D-Bus API Reference](docs/dbus-api.md) - Complete API specification
- [WiFi and VPN Usage Guide](docs/wifi-vpn-usage.md) - Usage examples with busctl and Python
- [Architecture Overview](docs/architecture.md) - System design and component interaction (coming soon)
