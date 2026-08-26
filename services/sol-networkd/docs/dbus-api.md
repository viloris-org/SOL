# sol-networkd D-Bus API Reference

## Service Information

- **Bus Name**: `org.sol.Network1`
- **Bus Type**: System bus
- **Main Object Path**: `/org/sol/Network1`

## Interfaces

### org.sol.Network1.Manager

Main network management interface.

**Object Path**: `/org/sol/Network1`

#### Methods

##### State() → s
Get current global network state.

**Returns**: String representation of network state
- `"Disconnected"` - No network connection
- `"Connecting"` - Establishing connection
- `"Connected"` - Connected with limited connectivity
- `"Online"` - Connected with full internet access

##### Connectivity() → u
Get connectivity state as integer.

**Returns**: Connectivity level
- `0` - No connectivity
- `1` - Captive portal detected
- `2` - Limited connectivity
- `3` - Full connectivity

##### ListDevices() → ao
List all network devices.

**Returns**: Array of object paths to device objects

##### ConnectToProfile(s) → o
Connect to a saved network profile.

**Parameters**:
- `profile_id` (s) - Profile identifier

**Returns**: Object path to active connection

**Errors**:
- `org.freedesktop.DBus.Error.Failed` - Connection failed

##### DisconnectProfile(s) → ()
Disconnect an active profile.

**Parameters**:
- `profile_id` (s) - Profile identifier

**Errors**:
- `org.freedesktop.DBus.Error.Failed` - Disconnection failed

##### ListProfiles() → ao
List all saved network profiles.

**Returns**: Array of object paths to profile objects

---

### org.sol.Network1.WiFi

WiFi-specific interface for scanning and connecting to wireless networks.

**Object Path**: `/org/sol/Network1/WiFi` (global) or per-device paths

#### Methods

##### Scan() → ()
Trigger a WiFi network scan.

**Note**: Results are available via GetNetworks() after scan completes.

##### GetNetworks() → aa{sv}
Get list of available networks from last scan.

**Returns**: Array of dictionaries containing network information
- `ssid` (s) - Network name
- `bssid` (s) - Access point MAC address
- `signal_strength` (y) - Signal strength (0-100)
- `frequency` (u) - Frequency in MHz
- `security` (s) - Security type ("open", "wep", "wpa", "wpa2", "wpa3")

##### Connect(ss) → ()
Connect to a WiFi network.

**Parameters**:
- `ssid` (s) - Network SSID
- `passphrase` (s) - Network passphrase (empty string for open networks)

**Errors**:
- `org.freedesktop.DBus.Error.Failed` - Connection failed (wrong password, out of range, etc.)

##### Disconnect() → ()
Disconnect from current WiFi network.

#### Properties

##### SignalStrength (readable, y)
Current signal strength (0-100). Returns 0 if disconnected.

##### CurrentNetwork (readable, s)
SSID of currently connected network. Empty string if disconnected.

##### Enabled (readwrite, b)
WiFi radio enabled state. Set to `false` to disable WiFi radio.

---

### org.sol.Network1.VPN

VPN management interface supporting WireGuard, OpenVPN, and IPSec.

**Object Path**: `/org/sol/Network1/VPN`

#### Methods

##### Connect(s) → ()
Connect to a VPN profile.

**Parameters**:
- `profile_id` (s) - VPN profile identifier

**Errors**:
- `org.freedesktop.DBus.Error.Failed` - Connection failed

##### Disconnect(s) → ()
Disconnect from a VPN.

**Parameters**:
- `profile_id` (s) - VPN profile identifier

##### CreateWireGuardProfile(a{sv}) → s
Create a new WireGuard VPN profile.

**Parameters**: Dictionary with configuration
- `name` (s) - Profile name
- `private_key` (s) - WireGuard private key (base64)
- `address` (s) - Tunnel IP address with CIDR (e.g., "10.0.0.2/24")
- `dns` (s, optional) - DNS server address
- `mtu` (u, optional) - MTU size (default: 1420)
- `peer_public_key` (s) - Peer public key (base64)
- `peer_endpoint` (s, optional) - Peer endpoint (host:port)
- `peer_allowed_ips` (s, optional) - Allowed IPs (default: "0.0.0.0/0")
- `peer_persistent_keepalive` (u, optional) - Keepalive interval in seconds

**Returns**: Profile ID (UUID)

**Example**:
```
{
  "name": "Home VPN",
  "private_key": "yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=",
  "address": "10.0.0.2/24",
  "dns": "1.1.1.1",
  "peer_public_key": "HIgo9xNzJMWLKASShiTqIybxZ0U3wGLiUeJ1PKf8ykw=",
  "peer_endpoint": "vpn.example.com:51820",
  "peer_allowed_ips": "0.0.0.0/0"
}
```

##### ListProfiles() → as
List all VPN profile IDs.

**Returns**: Array of profile ID strings

##### GetStatus(s) → a{sv}
Get VPN connection status.

**Parameters**:
- `profile_id` (s) - VPN profile identifier

**Returns**: Dictionary with status information
- `connected` (b) - Connection state
- `ip_address` (s) - Tunnel IP address
- `bytes_sent` (t) - Bytes transmitted
- `bytes_received` (t) - Bytes received
- `last_handshake` (t) - Unix timestamp of last handshake (WireGuard only)

##### DeleteProfile(s) → ()
Delete a VPN profile.

**Parameters**:
- `profile_id` (s) - VPN profile identifier

---

### org.sol.Network1.Device

Per-device interface for network device management.

**Object Path**: `/org/sol/Network1/Device/{device_id}`

#### Methods

##### Scan() → ()
Device-specific scan (WiFi only).

##### GetNetworks() → aa{sv}
Get available networks for this device (WiFi only).

#### Properties

##### DeviceType (readable, s)
Device type: "wifi", "ethernet", or "vpn"

##### State (readable, s)
Current device state:
- "unavailable"
- "disconnected"
- "preparing"
- "configuring"
- "need_auth"
- "ip_config"
- "ip_check"
- "active"
- "deactivating"
- "failed"

##### Interface (readable, s)
Kernel network interface name (e.g., "wlan0", "eth0")

---

### org.sol.Network1.Profile

Per-profile interface for network profile management.

**Object Path**: `/org/sol/Network1/Profile/{profile_id}`

#### Methods

##### Connect() → o
Connect using this profile.

**Returns**: Object path to active connection

##### Disconnect() → ()
Disconnect this profile if active.

##### Delete() → ()
Delete this profile permanently.

#### Properties

##### Id (readable, s)
Profile unique identifier (UUID).

##### Name (readable, s)
Human-readable profile name.

##### ProfileType (readable, s)
Profile type: "wifi", "ethernet", or "vpn"

##### AutoConnect (readwrite, b)
Whether to auto-connect to this profile when available.

##### Metered (readwrite, b)
Whether this connection is metered (affects app behavior).

---

## Signals

### org.sol.Network1.Manager

#### StateChanged(s)
Emitted when global network state changes.

**Parameters**:
- `new_state` (s) - New state string

#### ConnectivityChanged(u)
Emitted when connectivity level changes.

**Parameters**:
- `new_level` (u) - New connectivity level

#### DeviceAdded(o)
Emitted when a network device is added.

**Parameters**:
- `device_path` (o) - Object path to new device

#### DeviceRemoved(o)
Emitted when a network device is removed.

**Parameters**:
- `device_path` (o) - Object path to removed device

### org.sol.Network1.WiFi

#### ScanComplete()
Emitted when a WiFi scan completes.

#### NetworkAdded(a{sv})
Emitted when a new network is detected.

**Parameters**:
- `network_info` (a{sv}) - Network information dictionary

### org.sol.Network1.VPN

#### ConnectionStateChanged(sb)
Emitted when VPN connection state changes.

**Parameters**:
- `profile_id` (s) - Profile identifier
- `connected` (b) - New connection state

---

## Error Codes

All methods may return these standard D-Bus errors:

- `org.freedesktop.DBus.Error.Failed` - Operation failed (includes error message)
- `org.freedesktop.DBus.Error.InvalidArgs` - Invalid arguments provided
- `org.freedesktop.DBus.Error.UnknownMethod` - Method not found
- `org.freedesktop.DBus.Error.UnknownObject` - Object path not found
- `org.freedesktop.DBus.Error.UnknownInterface` - Interface not found

## Type Notation

D-Bus types used in this document:

- `s` - String
- `b` - Boolean
- `y` - Byte (uint8)
- `u` - Unsigned 32-bit integer
- `t` - Unsigned 64-bit integer
- `o` - Object path
- `as` - Array of strings
- `ao` - Array of object paths
- `a{sv}` - Dictionary (string keys, variant values)
- `aa{sv}` - Array of dictionaries

## Examples

See [wifi-vpn-usage.md](./wifi-vpn-usage.md) for complete usage examples using `busctl` and Python.

## Introspection

All interfaces support D-Bus introspection. To view the complete interface definition:

```bash
busctl introspect org.sol.Network1 /org/sol/Network1
busctl introspect org.sol.Network1 /org/sol/Network1/WiFi
busctl introspect org.sol.Network1 /org/sol/Network1/VPN
```

## Version

API Version: 1.0 (Phase 1)

**Stability**: Unstable - API may change before 1.0 release

## See Also

- [Implementation Status](./implementation-status.md)
- [WiFi and VPN Usage Guide](./wifi-vpn-usage.md)
- [Network Management Overview](./network-management.md)
