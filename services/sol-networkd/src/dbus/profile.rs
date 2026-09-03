use zbus::interface;
use zbus::zvariant::OwnedObjectPath;

use crate::manager::NetworkManager;
use crate::profile::ProfileId;

/// D-Bus Profile interface implementation
pub struct ProfileInterface {
    profile_id: ProfileId,
    manager: NetworkManager,
}

impl ProfileInterface {
    pub fn new(profile_id: ProfileId, manager: NetworkManager) -> Self {
        Self {
            profile_id,
            manager,
        }
    }
}

#[interface(name = "org.sol.Network1.Profile")]
impl ProfileInterface {
    /// Profile ID
    async fn id(&self) -> String {
        self.profile_id.0.clone()
    }

    /// Profile name
    async fn name(&self) -> String {
        if let Ok(Some(profile)) = self.manager.get_profile(&self.profile_id).await {
            profile.name
        } else {
            self.profile_id.0.clone()
        }
    }

    /// Profile type (wifi, ethernet, vpn)
    async fn profile_type(&self) -> String {
        if let Ok(Some(profile)) = self.manager.get_profile(&self.profile_id).await {
            match profile.profile_type {
                crate::profile::ProfileType::WiFi(_) => "wifi".to_string(),
                crate::profile::ProfileType::Ethernet(_) => "ethernet".to_string(),
                crate::profile::ProfileType::Vpn(_) => "vpn".to_string(),
            }
        } else {
            "unknown".to_string()
        }
    }

    /// Auto-connect enabled
    #[zbus(property)]
    async fn auto_connect(&self) -> bool {
        if let Ok(Some(profile)) = self.manager.get_profile(&self.profile_id).await {
            profile.auto_connect
        } else {
            false
        }
    }

    #[zbus(property)]
    async fn set_auto_connect(&self, value: bool) -> zbus::Result<()> {
        self.manager
            .set_auto_connect(&self.profile_id, value)
            .await
            .map_err(|e| zbus::Error::Failure(e.to_string()))
    }

    /// Metered connection
    #[zbus(property)]
    async fn metered(&self) -> bool {
        if let Ok(Some(profile)) = self.manager.get_profile(&self.profile_id).await {
            profile.metered
        } else {
            false
        }
    }

    #[zbus(property)]
    async fn set_metered(&self, _value: bool) -> zbus::Result<()> {
        // TODO: Implement metered flag persistence
        Err(zbus::Error::Failure(
            "Setting metered flag not yet implemented".into(),
        ))
    }

    /// Connect using this profile
    async fn connect(&self) -> zbus::fdo::Result<OwnedObjectPath> {
        self.manager
            .connect_to_profile(&self.profile_id)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;

        let path = format!(
            "/org/sol/Network1/Connection/{}",
            sanitize_path_component(&self.profile_id.0)
        );
        zbus::zvariant::ObjectPath::try_from(path)
            .map(|p| p.into())
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    /// Disconnect this profile
    async fn disconnect(&self) -> zbus::fdo::Result<()> {
        self.manager
            .disconnect_profile(&self.profile_id)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    /// Delete this profile
    async fn delete(&self) -> zbus::fdo::Result<()> {
        self.manager
            .delete_profile(&self.profile_id)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
}

fn sanitize_path_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}
