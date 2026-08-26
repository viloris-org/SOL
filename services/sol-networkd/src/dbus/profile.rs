use zbus::interface;
use zbus::zvariant::OwnedObjectPath;

use crate::profile::{Profile, ProfileType};

/// D-Bus Profile interface implementation
pub struct ProfileInterface {
    profile: Profile,
}

impl ProfileInterface {
    pub fn new(profile: Profile) -> Self {
        Self { profile }
    }
}

#[interface(name = "org.sol.Network1.Profile")]
impl ProfileInterface {
    /// Profile ID
    async fn id(&self) -> String {
        self.profile.id.0.clone()
    }

    /// Profile name
    async fn name(&self) -> String {
        self.profile.name.clone()
    }

    /// Profile type (wifi, ethernet, vpn)
    async fn profile_type(&self) -> String {
        match &self.profile.profile_type {
            ProfileType::WiFi(_) => "wifi".to_string(),
            ProfileType::Ethernet(_) => "ethernet".to_string(),
            ProfileType::Vpn(_) => "vpn".to_string(),
        }
    }

    /// Auto-connect enabled
    #[dbus_interface(property)]
    async fn auto_connect(&self) -> bool {
        self.profile.auto_connect
    }

    #[dbus_interface(property)]
    async fn set_auto_connect(&mut self, value: bool) {
        self.profile.auto_connect = value;
        // TODO: Persist to disk
    }

    /// Metered connection
    #[dbus_interface(property)]
    async fn metered(&self) -> bool {
        self.profile.metered
    }

    #[dbus_interface(property)]
    async fn set_metered(&mut self, value: bool) {
        self.profile.metered = value;
        // TODO: Persist to disk
    }

    /// Connect using this profile
    async fn connect(&self) -> zbus::fdo::Result<OwnedObjectPath> {
        // TODO: Trigger connection
        Err(zbus::fdo::Error::NotSupported(
            "Connect not yet implemented".into(),
        ))
    }

    /// Disconnect this profile
    async fn disconnect(&self) -> zbus::fdo::Result<()> {
        // TODO: Trigger disconnection
        Err(zbus::fdo::Error::NotSupported(
            "Disconnect not yet implemented".into(),
        ))
    }

    /// Delete this profile
    async fn delete(&self) -> zbus::fdo::Result<()> {
        // TODO: Delete profile
        Err(zbus::fdo::Error::NotSupported(
            "Delete not yet implemented".into(),
        ))
    }
}
