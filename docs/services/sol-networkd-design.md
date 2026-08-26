# sol-networkd Design

## Overview

`sol-networkd` is SOL's network management service. It provides a modern, capability-based approach to network connectivity management for WiFi, Ethernet, and VPN connections.

## Design Principles

### 1. Declarative + State Machine Driven

Network management is declarative rather than imperative. Applications and users express their desired network state; the service manages the transition.

```rust
// Not: "connect to WiFi"
// But: "desired network state"
struct NetworkIntent {
    profile: ProfileId,
    priority: u8,
    constraints: Constraints, // e.g., prefer WiFi when available, use Ethernet to save data
}

enum ConnectionState {
    Idle,
    Scanning,
    Associating,
    Authenticating,
    Configuring,  // DHCP/static IP
    Connected,
    Disconnecting,
}
```

### 2. Async Event-Driven (No Polling)

Uses tokio + netlink for efficient kernel communication:

```rust
async fn monitor_devices() {
    let mut netlink = NetlinkSocket::connect()?;
    loop {
        match netlink.recv_event().await {
            Event::LinkUp(device) => handle_link_up(device).await,
            Event::NewAddress(addr) => update_state(addr).await,
            Event::RouteChanged => recalculate_routing().await,
        }
    }
}
```

### 3. Capability-Based Security

Consistent with SOL's SCP capability model, not traditional root-based permissions:

```rust
struct NetworkCapability {
    can_list_networks: bool,      // Scan visible networks
    can_read_profiles: bool,      // View saved networks
    can_modify_profiles: bool,    // Add/remove networks
    can_control_connection: bool, // Connect/disconnect
    can_view_secrets: bool,       // View passwords
}

// Settings app gets full capabilities
// Third-party apps only request can_list_networks
```

### 4. Layered Architecture

```
┌─────────────────────────────────────┐
│  D-Bus API (org.sol.Network1)       │ ← Shell, Settings, Portal
├─────────────────────────────────────┤
│  Connection Manager                  │ ← Policy: auto-connect, priority
│  - Profile store                     │
│  - Auto-connect logic                │
│  - Captive portal detection          │
├─────────────────────────────────────┤
│  Device Manager                      │ ← Device state machines
│  - WiFi (via iwd or nl80211)        │
│  - Ethernet                          │
│  - VPN                               │
├─────────────────────────────────────┤
│  Network Stack Integration           │
│  - DHCP client                       │
│  - systemd-resolved (DNS)            │
│  - Routing table                     │
└─────────────────────────────────────┘
```

## Key Features

### Intelligent Automation

```rust
struct AutoConnectPolicy {
    prefer_ethernet: bool,          // Wired first
    avoid_metered: Vec<AppProfile>, // Pause certain apps on metered networks
    location_based: bool,           // Auto-connect HomeWiFi at home
    time_based: bool,               // Auto-connect OfficeWiFi during work hours
}
```

### Fast Connection (<1s target)

- **Cached scan results** - No re-scanning every time
- **Pre-association** - Prepare next AP when roaming
- **Parallel DHCP** - Non-blocking operations

### Transparent State

```rust
#[derive(Debug, Clone)]
struct NetworkState {
    connected: bool,
    connection_type: ConnectionType, // WiFi, Ethernet, VPN
    signal_strength: Option<u8>,     // 0-100
    metered: bool,                   // Data cap?
    captive_portal: bool,            // Login required?
    ipv4: Option<Ipv4Addr>,
    ipv6: Option<Ipv6Addr>,
    dns: Vec<IpAddr>,
    throughput: Throughput,          // Real-time upload/download speed
}

// Push changes via D-Bus signal (no polling)
signal StateChanged(state: NetworkState);
```

### Credential Security

```rust
struct ProfileStore {
    storage: SecretService, // Integrate with Linux secret service API
}

// WiFi password example
async fn save_wifi_profile(ssid: &str, passphrase: &str) {
    let encrypted = secrets.encrypt(passphrase).await?;
    profiles.insert(ssid, Profile {
        ssid: ssid.to_string(),
        passphrase_encrypted: encrypted,
        auto_connect: true,
    });
}
```

## D-Bus API

### Manager Interface

```xml
<interface name="org.sol.Network1.Manager">
  <!-- Properties (subscribable) -->
  <property name="State" type="s" access="read"/>
  <property name="Connectivity" type="u" access="read"/> 
  <!-- 0=none, 1=portal, 2=limited, 3=full -->
  
  <!-- Methods -->
  <method name="ListDevices">
    <arg name="devices" type="ao" direction="out"/> <!-- object paths -->
  </method>
  
  <method name="ConnectToProfile">
    <arg name="profile_id" type="s" direction="in"/>
    <arg name="connection" type="o" direction="out"/>
  </method>
  
  <method name="DisconnectProfile">
    <arg name="profile_id" type="s" direction="in"/>
  </method>
  
  <method name="CreateProfile">
    <arg name="settings" type="a{sv}" direction="in"/>
    <arg name="profile" type="o" direction="out"/>
  </method>
  
  <method name="DeleteProfile">
    <arg name="profile_id" type="s" direction="in"/>
  </method>
  
  <!-- Signals -->
  <signal name="StateChanged">
    <arg name="new_state" type="a{sv}"/> <!-- structured state -->
  </signal>
  
  <signal name="ConnectivityChanged">
    <arg name="connectivity" type="u"/>
  </signal>
</interface>
```

### Device Interface

```xml
<interface name="org.sol.Network1.Device">
  <property name="Type" type="s" access="read"/> <!-- wifi, ethernet, vpn -->
  <property name="State" type="s" access="read"/>
  <property name="Interface" type="s" access="read"/> <!-- e.g., wlan0 -->
  
  <method name="Scan"/> <!-- WiFi only -->
  <method name="GetNetworks">
    <arg name="networks" type="aa{sv}" direction="out"/>
  </method>
</interface>
```

### Profile Interface

```xml
<interface name="org.sol.Network1.Profile">
  <property name="Id" type="s" access="read"/>
  <property name="Name" type="s" access="read"/>
  <property name="Type" type="s" access="read"/> <!-- wifi, ethernet, vpn -->
  <property name="AutoConnect" type="b" access="readwrite"/>
  <property name="Metered" type="b" access="readwrite"/>
  
  <method name="Connect">
    <arg name="connection" type="o" direction="out"/>
  </method>
  
  <method name="Disconnect"/>
  
  <method name="Delete"/>
</interface>
```

## Integration with SOL Ecosystem

### sol-portal
Handles captive portal detection and launches browser for login.

### sol-settingsd
Exposes network settings schema for Settings app:
- WiFi profiles
- VPN configurations
- Network preferences (auto-connect, metered)

### sol-shell
Displays network status icon:
- Connected/disconnected
- Signal strength
- Metered indicator
- VPN active

### SCP (Future)
Network capability in SCP for apps requesting network access.

## Technology Stack

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
zbus = "4"                          # D-Bus
netlink-packet-route = "0.17"       # Kernel netlink communication
rtnetlink = "0.14"                  # High-level netlink API

# WiFi backend
# Option 1: iwd (recommended, modern)
# Communicate with iwd via D-Bus, iwd handles low-level nl80211

# Option 2: Direct nl80211
# nl80211 = "0.1"                   # Full control, more work

# DHCP
dhcproto = "0.9"                    # DHCP protocol parsing/building

# DNS
trust-dns-resolver = "0.23"

# Encryption
ring = "0.17"                       # Credential encryption

# Logging
tracing = "0.1"
tracing-subscriber = "0.3"
```

## Module Structure

```
services/sol-networkd/
├── src/
│   ├── main.rs                    # Entry point, D-Bus service setup
│   ├── manager.rs                 # Connection manager, auto-connect logic
│   ├── device/
│   │   ├── mod.rs                 # Device manager
│   │   ├── wifi.rs                # WiFi device implementation
│   │   ├── ethernet.rs            # Ethernet device implementation
│   │   └── vpn.rs                 # VPN device implementation
│   ├── profile/
│   │   ├── mod.rs                 # Profile store
│   │   ├── wifi_profile.rs        # WiFi profile
│   │   ├── ethernet_profile.rs    # Ethernet profile
│   │   └── vpn_profile.rs         # VPN profile
│   ├── netlink/
│   │   ├── mod.rs                 # Netlink event loop
│   │   └── monitor.rs             # Device/route/address monitoring
│   ├── dhcp.rs                    # DHCP client
│   ├── dns.rs                     # DNS integration (systemd-resolved)
│   ├── captive_portal.rs          # Captive portal detection
│   ├── dbus/
│   │   ├── mod.rs                 # D-Bus interface implementations
│   │   ├── manager.rs             # Manager interface
│   │   ├── device.rs              # Device interface
│   │   └── profile.rs             # Profile interface
│   └── security.rs                # Credential encryption/storage
├── tests/
│   ├── manager_tests.rs
│   ├── wifi_tests.rs
│   └── profile_tests.rs
└── Cargo.toml
```

## Testing Strategy

### Unit Tests

```rust
#[tokio::test]
async fn test_wifi_connection_flow() {
    let mock_netlink = MockNetlink::new();
    let service = NetworkService::new(mock_netlink);
    
    // Simulate scan results
    mock_netlink.emit(Event::NewScanResults(vec![
        Network { ssid: "TestWiFi", signal: 80 }
    ]));
    
    // Request connection
    let result = service.connect("TestWiFi", "password").await;
    
    // Verify state transition
    assert_eq!(service.state(), ConnectionState::Connected);
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_dbus_api() {
    let service = spawn_networkd().await;
    let conn = zbus::Connection::system().await?;
    let proxy = NetworkManagerProxy::new(&conn).await?;
    
    // List devices
    let devices = proxy.list_devices().await?;
    assert!(!devices.is_empty());
    
    // Monitor state changes
    let mut stream = proxy.receive_state_changed().await?;
    // ... test state change signal
}
```

## Implementation Phases

### Phase 0 (Current)
Not needed - rely on host system network management.

### Phase 1 (Real Hardware Boot)
**Minimal viable network service:**
- Ethernet device support
- Static IP configuration
- Basic WiFi support (connect to known network)
- D-Bus API for Settings app

### Phase 2
**Full-featured networking:**
- WiFi scanning and profile management
- Auto-connect policies
- DHCP client
- Captive portal detection
- VPN support (WireGuard)

### Phase 3
**Advanced features:**
- Location-based auto-connect
- Network quality monitoring
- Bandwidth limiting per app
- Hotspot mode

## Security Considerations

1. **Credential Storage**: WiFi passwords encrypted at rest using Linux secret service API
2. **D-Bus Policy**: Only Settings app can modify profiles; regular apps can only list networks
3. **Audit Logging**: All network changes logged for security review
4. **Capability Enforcement**: Apps declare required network capabilities; user approves
5. **VPN Leak Prevention**: Kill-switch for VPN to prevent traffic leaks

## Performance Goals

- **Connection time**: <1s for known networks
- **Scan time**: <3s for WiFi scan
- **Memory footprint**: <20MB resident
- **CPU usage**: <1% when idle, <5% during active connection

## Future Considerations

- **Mobile network support**: 4G/5G modems
- **Bluetooth tethering**: Use phone as network source
- **Network namespaces**: Per-app network isolation
- **Zero-conf networking**: mDNS/DNS-SD for local device discovery
