# WiFi and VPN Usage Guide

This document provides examples of using sol-networkd's WiFi and VPN features via D-Bus.

## WiFi Management

### Scanning for Networks

```bash
# Trigger a WiFi scan
busctl call org.sol.Network1 /org/sol/Network1/WiFi org.sol.Network1.WiFi Scan

# Get list of available networks
busctl call org.sol.Network1 /org/sol/Network1/WiFi org.sol.Network1.WiFi GetNetworks
```

### Connecting to WiFi

```bash
# Connect to an open network
busctl call org.sol.Network1 /org/sol/Network1/WiFi org.sol.Network1.WiFi \
    Connect ss "MyNetwork" ""

# Connect to a secured network
busctl call org.sol.Network1 /org/sol/Network1/WiFi org.sol.Network1.WiFi \
    Connect ss "MySecureNetwork" "my-password"
```

### WiFi Properties

```bash
# Get current signal strength
busctl get-property org.sol.Network1 /org/sol/Network1/WiFi \
    org.sol.Network1.WiFi SignalStrength

# Get currently connected network
busctl get-property org.sol.Network1 /org/sol/Network1/WiFi \
    org.sol.Network1.WiFi CurrentNetwork

# Check if WiFi is enabled
busctl get-property org.sol.Network1 /org/sol/Network1/WiFi \
    org.sol.Network1.WiFi Enabled

# Enable/disable WiFi
busctl set-property org.sol.Network1 /org/sol/Network1/WiFi \
    org.sol.Network1.WiFi Enabled b true
```

### Disconnecting

```bash
# Disconnect from current WiFi network
busctl call org.sol.Network1 /org/sol/Network1/WiFi org.sol.Network1.WiFi Disconnect
```

## VPN Management

### WireGuard VPN

```bash
# Create a WireGuard VPN profile
busctl call org.sol.Network1 /org/sol/Network1/VPN org.sol.Network1.VPN \
    CreateWireGuardProfile 'a{sv}' 6 \
        name s "Home VPN" \
        private_key s "your-private-key" \
        address s "10.0.0.2/24" \
        dns s "1.1.1.1" \
        peer_public_key s "server-public-key" \
        peer_endpoint s "vpn.example.com:51820"

# Connect to VPN
busctl call org.sol.Network1 /org/sol/Network1/VPN org.sol.Network1.VPN \
    Connect s "profile-id"

# Disconnect from VPN
busctl call org.sol.Network1 /org/sol/Network1/VPN org.sol.Network1.VPN \
    Disconnect s "profile-id"
```

### Managing VPN Profiles

```bash
# List all VPN profiles
busctl call org.sol.Network1 /org/sol/Network1/VPN org.sol.Network1.VPN \
    ListProfiles

# Get VPN status
busctl call org.sol.Network1 /org/sol/Network1/VPN org.sol.Network1.VPN \
    GetStatus s "profile-id"

# Delete a VPN profile
busctl call org.sol.Network1 /org/sol/Network1/VPN org.sol.Network1.VPN \
    DeleteProfile s "profile-id"
```

## Python Example

```python
from gi.repository import Gio

# Connect to D-Bus
bus = Gio.bus_get_sync(Gio.BusType.SYSTEM, None)

# WiFi operations
wifi = Gio.DBusProxy.new_sync(
    bus,
    Gio.DBusProxyFlags.NONE,
    None,
    'org.sol.Network1',
    '/org/sol/Network1/WiFi',
    'org.sol.Network1.WiFi',
    None
)

# Scan for networks
wifi.call_sync('Scan', None, Gio.DBusCallFlags.NONE, -1, None)

# Get networks
networks = wifi.call_sync('GetNetworks', None, Gio.DBusCallFlags.NONE, -1, None)
for network in networks[0]:
    print(f"SSID: {network['ssid']}, Signal: {network['signal_strength']}%")

# Connect to network
wifi.call_sync(
    'Connect',
    GLib.Variant('(ss)', ('MyNetwork', 'password')),
    Gio.DBusCallFlags.NONE,
    -1,
    None
)

# VPN operations
vpn = Gio.DBusProxy.new_sync(
    bus,
    Gio.DBusProxyFlags.NONE,
    None,
    'org.sol.Network1',
    '/org/sol/Network1/VPN',
    'org.sol.Network1.VPN',
    None
)

# Create WireGuard profile
vpn.call_sync(
    'CreateWireGuardProfile',
    GLib.Variant('(a{sv})', ({
        'name': GLib.Variant('s', 'Home VPN'),
        'private_key': GLib.Variant('s', 'your-private-key'),
        'address': GLib.Variant('s', '10.0.0.2/24'),
        'peer_public_key': GLib.Variant('s', 'server-public-key'),
        'peer_endpoint': GLib.Variant('s', 'vpn.example.com:51820'),
    },)),
    Gio.DBusCallFlags.NONE,
    -1,
    None
)

# Connect to VPN
profile_id = 'your-profile-id'
vpn.call_sync(
    'Connect',
    GLib.Variant('(s)', (profile_id,)),
    Gio.DBusCallFlags.NONE,
    -1,
    None
)
```

## Integration with sol-settings

The WiFi and VPN interfaces are designed to integrate with sol-settings UI:

1. **WiFi Panel**: Shows available networks, signal strength, connection status
2. **VPN Panel**: Lists configured VPN profiles, allows adding new profiles
3. **Network Status**: Shows active connections in system status area

## Implementation Status

### Completed (Phase 1)
- D-Bus interface definitions
- Basic structure for WiFi scanning and connection
- WireGuard VPN profile management
- Integration with NetworkManager

### TODO (Phase 2)
- Actual iwd integration for WiFi operations
- WireGuard kernel interface configuration
- Network state persistence
- Captive portal handling
- Connection auto-retry logic
- Signal strength monitoring
- Network metrics collection

## Architecture Notes

### WiFi Backend (iwd)
- Uses Intel's iwd (iNet Wireless Daemon) for WiFi management
- Communicates via D-Bus interface `net.connman.iwd`
- Provides better performance and security than wpa_supplicant

### VPN Backend (WireGuard)
- Uses WireGuard kernel module for VPN tunneling
- Configuration via `wireguard-control` crate
- Supports modern cryptography (Curve25519, ChaCha20, Poly1305)

### Security
- WiFi passphrases encrypted at rest using platform keyring
- VPN private keys stored securely
- Connection credentials never logged
- Capability-based access control via SCP

## See Also

- [Network Management Overview](./network-management.md)
- [D-Bus API Reference](./dbus-api.md)
- [Security Model](./security.md)
