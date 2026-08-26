# sol-networkd WiFi and VPN Support

This document demonstrates how to use the WiFi and VPN features in sol-networkd.

## WiFi Management

### D-Bus Interface: `org.sol.Network1.WiFi`

The WiFi interface provides methods for scanning, connecting, and managing WiFi networks.

#### Scan for Networks

```bash
# Using busctl
busctl call org.sol.Network1 /org/sol/Network1/WiFi org.sol.Network1.WiFi Scan
```

#### Get Available Networks

```bash
# List scanned networks
busctl call org.sol.Network1 /org/sol/Network1/WiFi org.sol.Network1.WiFi GetNetworks
```

Returns an array of dictionaries with network information:
- `ssid`: Network name
- `bssid`: MAC address of access point
- `signal_strength`: Signal quality (0-100)
- `frequency`: WiFi frequency in MHz
- `security`: Security type (Open, WEP, WPA, WPA2, WPA3)

#### Connect to Network

```bash
# Connect to WPA2 network
busctl call org.sol.Network1 /org/sol/Network1/WiFi org.sol.Network1.WiFi Connect ss "MyNetwork" "mypassword"

# Connect to open network
busctl call org.sol.Network1 /org/sol/Network1/WiFi org.sol.Network1.WiFi Connect ss "OpenWiFi" ""
```

#### Disconnect

```bash
busctl call org.sol.Network1 /org/sol/Network1/WiFi org.sol.Network1.WiFi Disconnect
```

#### Check Signal Strength

```bash
busctl get-property org.sol.Network1 /org/sol/Network1/WiFi org.sol.Network1.WiFi SignalStrength
```

#### Get Current Network

```bash
busctl get-property org.sol.Network1 /org/sol/Network1/WiFi org.sol.Network1.WiFi CurrentNetwork
```

## VPN Management

### D-Bus Interface: `org.sol.Network1.VPN`

The VPN interface currently supports WireGuard VPNs, with OpenVPN and IPSec planned.

#### Generate WireGuard Keypair

```bash
# Generate a new private/public keypair
busctl call org.sol.Network1 /org/sol/Network1/VPN org.sol.Network1.VPN GenerateKeypair
```

Returns: `(private_key, public_key)` as base64-encoded strings

#### Create WireGuard VPN

```bash
# Example using Python and dbus-python
import dbus

bus = dbus.SystemBus()
vpn = bus.get_object('org.sol.Network1', '/org/sol/Network1/VPN')
vpn_interface = dbus.Interface(vpn, 'org.sol.Network1.VPN')

config = {
    'private_key': 'YOUR_PRIVATE_KEY_BASE64',
    'address': '10.0.0.2/24',
    'listen_port': 51820,
    'peers': [
        {
            'public_key': 'SERVER_PUBLIC_KEY_BASE64',
            'endpoint': 'vpn.example.com:51820',
            'allowed_ips': ['0.0.0.0/0'],
            'persistent_keepalive': 25
        }
    ]
}

profile_id = vpn_interface.CreateWireguard('My VPN', config)
print(f"Created VPN profile: {profile_id}")
```

#### Connect to VPN

```bash
busctl call org.sol.Network1 /org/sol/Network1/VPN org.sol.Network1.VPN Connect s "PROFILE_ID"
```

#### Disconnect VPN

```bash
busctl call org.sol.Network1 /org/sol/Network1/VPN org.sol.Network1.VPN Disconnect s "PROFILE_ID"
```

#### Get VPN Status

```bash
busctl call org.sol.Network1 /org/sol/Network1/VPN org.sol.Network1.VPN GetStatus s "PROFILE_ID"
```

Returns connection status and peer statistics:
- `connected`: boolean
- `peers`: array of peer status (public_key, endpoint, rx_bytes, tx_bytes, last_handshake)

#### List VPN Profiles

```bash
busctl call org.sol.Network1 /org/sol/Network1/VPN org.sol.Network1.VPN ListProfiles
```

## Implementation Details

### WiFi Backend: iwd (Intel Wireless Daemon)

sol-networkd uses iwd for WiFi management:

- **Scan**: Triggers `net.connman.iwd.Station.Scan`
- **Networks**: Retrieved via `net.connman.iwd.Station.GetOrderedNetworks`
- **Connect**: Uses `net.connman.iwd.Station.Connect` with stored credentials
- **Credentials**: Stored in `/var/lib/iwd/<ssid>.psk`

#### Requirements

iwd must be running:
```bash
systemctl start iwd.service
systemctl enable iwd.service
```

### VPN Backend: WireGuard

sol-networkd uses the `wireguard-control` crate for kernel WireGuard:

- **Interface**: Creates/configures kernel WireGuard interfaces
- **Peers**: Manages peer configurations and allowed IPs
- **Status**: Reads handshake times and traffic statistics

#### Requirements

WireGuard kernel module must be loaded:
```bash
modprobe wireguard
```

## Security

### Credential Storage

- WiFi passphrases: Stored in iwd's format at `/var/lib/iwd/`
- VPN keys: Encrypted with system key (TODO: implement actual encryption)
- D-Bus access: Restricted to privileged users via PolicyKit

### Kill Switch

VPN profiles support a "kill switch" feature that prevents traffic leaks if the VPN disconnects:

```python
# Enable kill switch on VPN profile
profile = {
    'name': 'Secure VPN',
    'vpn_type': {...},
    'kill_switch': True,  # Block traffic if VPN drops
}
```

When enabled, firewall rules ensure no traffic escapes outside the VPN tunnel.

## Future Enhancements

- OpenVPN support
- IPSec/IKEv2 support
- WPA3 Enterprise (802.1X)
- WiFi Direct
- Hotspot/AP mode
- VPN split-tunneling
- Automatic VPN on untrusted networks
