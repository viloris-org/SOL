use std::collections::HashMap;
use zbus::{fdo, interface};

use crate::device::vpn::{WireGuardConfig, WireGuardPeer};
use crate::manager::NetworkManager;

/// D-Bus VPN interface implementation
pub struct VpnInterface {
    manager: NetworkManager,
}

impl VpnInterface {
    pub fn new(manager: NetworkManager) -> Self {
        Self { manager }
    }
}

#[interface(name = "org.sol.Network1.VPN")]
impl VpnInterface {
    /// Connect to a VPN
    ///
    /// # Arguments
    /// * `profile_id` - VPN profile ID to connect
    async fn connect(&self, profile_id: String) -> fdo::Result<()> {
        self.manager
            .connect_to_profile(&crate::profile::ProfileId(profile_id))
            .await
            .map_err(|e| fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }

    /// Disconnect from current VPN
    ///
    /// # Arguments
    /// * `profile_id` - VPN profile ID to disconnect
    async fn disconnect(&self, profile_id: String) -> fdo::Result<()> {
        self.manager
            .disconnect_profile(&crate::profile::ProfileId(profile_id))
            .await
            .map_err(|e| fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }

    /// Create a new WireGuard VPN configuration
    ///
    /// # Arguments
    /// * `name` - VPN connection name
    /// * `config` - WireGuard configuration as a dict
    async fn create_wireguard(
        &self,
        name: String,
        config: HashMap<String, zbus::zvariant::Value<'_>>,
    ) -> fdo::Result<String> {
        // Parse WireGuard config from dict
        let private_key = config
            .get("private_key")
            .and_then(|v| v.try_to_owned().ok())
            .and_then(|v| v.try_into().ok())
            .ok_or_else(|| fdo::Error::InvalidArgs("Missing private_key".into()))?;

        let address = config
            .get("address")
            .and_then(|v| v.try_to_owned().ok())
            .and_then(|v| v.try_into().ok());

        let listen_port = config
            .get("listen_port")
            .and_then(|v| v.try_to_owned().ok())
            .and_then(|v| v.try_into().ok());

        // Parse peers
        let peers_value = config
            .get("peers")
            .ok_or_else(|| fdo::Error::InvalidArgs("Missing peers".into()))?;

        let peers = parse_peers(peers_value)
            .map_err(|e| fdo::Error::InvalidArgs(format!("Invalid peers: {}", e)))?;

        let mut wg_config = WireGuardConfig::new(private_key);
        if let Some(addr) = address {
            wg_config = wg_config.set_address(addr);
        }
        if let Some(port) = listen_port {
            wg_config = wg_config.set_listen_port(port);
        }
        for peer in peers {
            wg_config = wg_config.add_peer(peer);
        }

        // Create profile
        let profile = crate::profile::Profile {
            id: crate::profile::ProfileId(uuid::Uuid::new_v4().to_string()),
            name: name.clone(),
            auto_connect: false,
            metered: false,
            profile_type: crate::profile::ProfileType::Vpn(
                crate::profile::vpn_profile::VpnProfile {
                    name: name.clone(),
                    vpn_type: crate::profile::vpn_profile::VpnType::WireGuard(
                        wireguard_config_to_profile(&wg_config),
                    ),
                    auto_connect: false,
                    kill_switch: true,
                    on_demand: false,
                },
            ),
        };

        let profile_id = self
            .manager
            .create_profile(profile)
            .await
            .map_err(|e| fdo::Error::Failed(e.to_string()))?;

        Ok(profile_id.0)
    }

    /// Generate a new WireGuard keypair
    async fn generate_keypair(&self) -> fdo::Result<(String, String)> {
        let (private_key, public_key) = crate::device::vpn::generate_keypair()
            .map_err(|e| fdo::Error::Failed(e.to_string()))?;
        Ok((private_key, public_key))
    }

    /// Get VPN connection status
    ///
    /// # Arguments
    /// * `profile_id` - VPN profile ID
    async fn get_status(
        &self,
        _profile_id: String,
    ) -> fdo::Result<HashMap<String, zbus::zvariant::Value<'static>>> {
        // TODO: Get actual VPN status
        let mut status = HashMap::new();
        status.insert("connected".to_string(), false.into());
        Ok(status)
    }

    /// List available VPN profiles
    async fn list_profiles(&self) -> fdo::Result<Vec<String>> {
        let profiles = self.manager.list_profiles().await;
        Ok(profiles.into_iter().map(|p| p.0).collect())
    }
}

fn parse_peers(value: &zbus::zvariant::Value) -> Result<Vec<WireGuardPeer>, String> {
    let array = value
        .downcast_ref::<zbus::zvariant::Array>()
        .map_err(|_| "peers must be an array".to_string())?;

    let mut peers = Vec::new();
    for item in array.iter() {
        let dict = item
            .downcast_ref::<zbus::zvariant::Dict>()
            .map_err(|_| "each peer must be a dict".to_string())?;

        let mut peer_map = HashMap::new();
        for (k, v) in dict.iter() {
            let key: &str = k.try_into().map_err(|_| "key must be string".to_string())?;
            peer_map.insert(key.to_string(), v);
        }

        let public_key = peer_map
            .get("public_key")
            .and_then(|v| {
                if let zbus::zvariant::Value::Str(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .ok_or("missing public_key")?
            .to_string();

        let endpoint = peer_map.get("endpoint").and_then(|v| {
            if let zbus::zvariant::Value::Str(s) = v {
                Some(s.as_str().to_string())
            } else {
                None
            }
        });

        let allowed_ips = peer_map
            .get("allowed_ips")
            .and_then(|v| v.downcast_ref::<zbus::zvariant::Array>().ok())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| {
                        if let zbus::zvariant::Value::Str(s) = v {
                            Some(s.as_str().to_string())
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_else(|| vec!["0.0.0.0/0".to_string()]);

        let persistent_keepalive = peer_map
            .get("persistent_keepalive")
            .and_then(|v| v.downcast_ref::<u16>().ok());

        peers.push(WireGuardPeer {
            public_key,
            preshared_key: None,
            endpoint,
            allowed_ips,
            persistent_keepalive,
        });
    }

    Ok(peers)
}

fn wireguard_config_to_profile(
    config: &WireGuardConfig,
) -> crate::profile::vpn_profile::WireGuardProfile {
    use crate::profile::vpn_profile::{WireGuardPeerProfile, WireGuardProfile};

    WireGuardProfile {
        private_key_encrypted: config.private_key.as_bytes().to_vec(), // TODO: Actually encrypt
        address: config.address.clone(),
        listen_port: config.listen_port,
        peers: config
            .peers
            .iter()
            .map(|p| WireGuardPeerProfile {
                name: None,
                public_key: p.public_key.clone(),
                preshared_key_encrypted: p.preshared_key.as_ref().map(|k| k.as_bytes().to_vec()),
                endpoint: p.endpoint.clone(),
                allowed_ips: p.allowed_ips.clone(),
                persistent_keepalive: p.persistent_keepalive,
            })
            .collect(),
        dns: config.dns.clone(),
        mtu: Some(1420),
    }
}
