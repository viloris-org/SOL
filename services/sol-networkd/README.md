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
- D-Bus service on `org.sol.Network1` with Manager/Device/Profile interfaces
- Netlink integration via rtnetlink for device monitoring
- Device abstraction (WiFi/Ethernet/VPN types with per-type handlers)

**Device Management**
- Interface discovery via netlink + /sys/class/net
- Device state tracking (Unavailable → Active lifecycle)
- Per-device-type specialization (WiFiDevice, EthernetDevice, VpnDevice scaffolds)

**Profile System**
- Connection profile storage (WiFi/Ethernet/VPN)
- Profile-to-device activation flow
- Auto-connect policy framework
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

### 🚧 Phase 2 Pending

**WiFi**
- nl80211 scanning implementation
- WPA2/WPA3 authentication via wpa_supplicant or iwd
- Signal strength monitoring
- Network switching logic

**VPN**
- WireGuard integration
- OpenVPN support
- Split tunneling
- DNS leak prevention

**Profile Persistence**
- Profile file I/O (JSON or TOML)
- Profile validation on load
- Migration on schema changes

**Advanced Features**
- IPv6 support (DHCPv6, SLAAC)
- Connection prioritization (prefer ethernet, avoid metered)
- Time/location-based auto-connect policies
- Hotspot mode

## D-Bus API

### org.sol.Network1.Manager

**Methods:**
- `ListDevices() -> a(so)` - Returns (index, object_path) pairs
- `GetDevice(o device) -> a{sv}` - Device properties dict
- `ListConnections() -> ao` - Active connection object paths
- `ActivateConnection(s profile_id, o device) -> o` - Returns connection path
- `AddProfile(a{sv} profile) -> s` - Returns profile_id
- `GetProfiles() -> as` - Returns profile_id list
- `CheckConnectivity() -> u` - Returns PortalState enum

**Properties:**
- `State: u` - NetworkState enum (Unknown/Disconnected/Connected/Limited)
- `Connectivity: u` - PortalState enum (from captive portal check)
- `PrimaryConnection: o` - Object path of active connection

**Signals:**
- `DeviceAdded(o device)` - New device detected
- `DeviceRemoved(o device)` - Device removed
- `StateChanged(u new_state)` - Network state transition
- `ConnectivityChanged(u new_state)` - Portal detection result

### org.sol.Network1.Device

**Properties:**
- `Type: s` - "wifi", "ethernet", "vpn"
- `State: s` - Device state string
- `Interface: s` - Linux interface name (wlan0, eth0, etc.)

**Methods (WiFi-specific):**
- `Scan() -> ()`
- `GetNetworks() -> aa{sv}` - Returns SSID/signal/security dicts

### org.sol.Network1.Profile

**Properties:**
- `Id: s` - Profile UUID
- `Name: s` - User-visible name
- `Type: s` - "wifi", "ethernet", "vpn"
- `AutoConnect: b` - Auto-connect on availability
- `Metered: b` - Treat as metered connection

**Methods:**
- `Connect() -> o` - Activate and return connection path
- `Disconnect() -> ()`
- `Delete() -> ()`

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
- **tokio** - Async runtime
- **anyhow** - Error handling

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

- [ ] WiFi nl80211 scanning and authentication
- [ ] WireGuard VPN integration
- [ ] Profile file persistence
- [ ] IPv6 DHCPv6 and SLAAC
- [ ] Hotspot mode (AP + DHCP server)
- [ ] Connection statistics and usage tracking
- [ ] Network cost awareness (prefer WiFi over cellular)
- [ ] Captive portal web view integration
- [ ] MAC address randomization
