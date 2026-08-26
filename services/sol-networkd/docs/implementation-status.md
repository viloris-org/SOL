# sol-networkd Implementation Status

## Overview

sol-networkd is SOL's network management service, providing WiFi, Ethernet, VPN, and general network connectivity management through a D-Bus API.

## Phase 1: Complete ✓

### Core Architecture
- [x] NetworkManager core state machine
- [x] Device abstraction layer (WiFi, Ethernet, VPN)
- [x] Profile storage and management
- [x] D-Bus service interfaces
- [x] Network state tracking
- [x] Connectivity checking via captive portal detection

### WiFi Management
- [x] WiFi device abstraction
- [x] D-Bus WiFi interface (`org.sol.Network1.WiFi`)
- [x] Network scanning API
- [x] Connection/disconnection methods
- [x] Signal strength property
- [x] Current network property
- [x] Enable/disable WiFi
- [x] iwd integration structure (ready for backend connection)

### VPN Support
- [x] VPN device abstraction
- [x] WireGuard configuration structures
- [x] D-Bus VPN interface (`org.sol.Network1.VPN`)
- [x] Profile management (create, list, delete)
- [x] Connection/disconnection methods
- [x] Multi-peer WireGuard support
- [x] VPN status reporting

### Network Profiles
- [x] Profile store with persistence
- [x] WiFi profiles (SSID, security, passphrase)
- [x] Ethernet profiles (DHCP/static IP)
- [x] VPN profiles (WireGuard, OpenVPN, IPSec structures)
- [x] Auto-connect policy framework
- [x] Metered connection tracking

### Low-Level Networking
- [x] Netlink monitoring for link state changes
- [x] DHCP client implementation (discover, request, renew, release)
- [x] DNS management (systemd-resolved integration)
- [x] IP address configuration

### Security
- [x] Secret storage for WiFi passphrases
- [x] VPN private key management
- [x] Encryption at rest using platform keyring
- [x] Secure credential handling

### D-Bus API
- [x] Manager interface (`org.sol.Network1.Manager`)
  - State reporting
  - Connectivity checking
  - Device enumeration
  - Profile connection/disconnection
- [x] Device interface (`org.sol.Network1.Device`)
  - Per-device properties and methods
- [x] WiFi interface (`org.sol.Network1.WiFi`)
  - Scan, connect, disconnect
  - Properties (signal strength, current network, enabled)
- [x] VPN interface (`org.sol.Network1.VPN`)
  - Profile management
  - Connect/disconnect
  - Status reporting
- [x] Profile interface (`org.sol.Network1.Profile`)
  - Profile properties
  - Auto-connect settings

## Phase 2: Planned

### WiFi Backend Integration
- [ ] Complete iwd D-Bus integration
- [ ] Real-time signal strength monitoring
- [ ] Network quality metrics (throughput, latency)
- [ ] Background scanning
- [ ] Roaming support
- [ ] Hidden network support
- [ ] WPS (WiFi Protected Setup)

### VPN Backend Integration
- [ ] WireGuard kernel module interaction via wireguard-control
- [ ] Interface creation and configuration
- [ ] Routing table management
- [ ] Split tunneling support
- [ ] OpenVPN backend (via openvpn binary)
- [ ] IPSec/IKEv2 backend (via strongSwan)
- [ ] Automatic reconnection on network changes

### Advanced Features
- [ ] Network Time Security (NTS) client
- [ ] mDNS/DNS-SD service discovery
- [ ] IPv6 support (SLAAC, DHCPv6)
- [ ] 802.1X authentication for enterprise WiFi
- [ ] Hotspot/AP mode
- [ ] Network usage statistics
- [ ] Traffic shaping/QoS
- [ ] Firewall integration

### Connectivity
- [ ] Captive portal auto-detection and handling
- [ ] Fallback connection ordering (prefer Ethernet > WiFi > cellular)
- [ ] Connection retry with exponential backoff
- [ ] Network health monitoring
- [ ] Automatic portal authentication for known networks

### Integration
- [ ] sol-settings UI integration
- [ ] Shell network indicator
- [ ] Notification service integration (connection status)
- [ ] Power management coordination (suspend/resume)
- [ ] Airplane mode support

### Testing
- [ ] Unit tests for all modules
- [ ] Integration tests with mock backends
- [ ] End-to-end tests with real hardware
- [ ] Performance benchmarks
- [ ] Security audit

## Phase 3: Future Enhancements

- [ ] Mobile broadband (4G/5G) support
- [ ] Bluetooth tethering
- [ ] Ethernet bonding/teaming
- [ ] VLANs
- [ ] Bridge interfaces
- [ ] Network namespaces for containers
- [ ] Per-app network policies
- [ ] Advanced routing (policy routing, source routing)
- [ ] IPv6 privacy extensions
- [ ] DNSSEC validation
- [ ] DoT (DNS over TLS) / DoH (DNS over HTTPS)

## Technology Stack

### Core Libraries
- **zbus**: D-Bus communication
- **tokio**: Async runtime
- **netlink**: Low-level network monitoring
- **dhcproto**: DHCP protocol implementation
- **reqwest**: HTTP client for connectivity checking
- **wireguard-control**: WireGuard configuration

### External Dependencies
- **iwd**: WiFi management (communicates via D-Bus)
- **systemd-resolved**: DNS resolution
- **WireGuard kernel module**: VPN tunneling

### Security
- **ring**: Cryptographic operations
- **Platform keyring**: Secure credential storage

## API Stability

### Stable (v1.0)
- Core D-Bus interfaces
- Profile format
- Basic WiFi/Ethernet operations

### Unstable (under development)
- VPN backend specifics
- Advanced WiFi features
- Network metrics API

## Documentation

- [Architecture Overview](./architecture.md)
- [D-Bus API Reference](./dbus-api.md)
- [WiFi and VPN Usage](./wifi-vpn-usage.md)
- [Security Model](./security.md)

## Contributing

When adding new features:
1. Update this status document
2. Add D-Bus interface changes to API reference
3. Write usage examples
4. Add unit and integration tests
5. Update ROADMAP.md if it's a significant feature

## Notes

- This is a **native SOL service** - not a compatibility layer
- Designed for modern hardware (2020+)
- Security and usability over feature parity with NetworkManager
- Integration with SCP capability model for app network access
