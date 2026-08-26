pub mod wifi_profile;
pub mod ethernet_profile;
pub mod vpn_profile;

use std::collections::HashMap;
use std::fmt;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::security::SecretStore;

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProfileId(pub String);

impl fmt::Display for ProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone)]
pub struct Profile {
    pub id: ProfileId,
    pub name: String,
    pub profile_type: ProfileType,
    pub auto_connect: bool,
    pub metered: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProfileType {
    WiFi(wifi_profile::WiFiProfile),
    Ethernet(ethernet_profile::EthernetProfile),
    Vpn(vpn_profile::VpnProfile),
}

/// Profile storage and management
pub struct ProfileStore {
    profiles: HashMap<ProfileId, Profile>,
    secret_store: SecretStore,
}

impl ProfileStore {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            profiles: HashMap::new(),
            secret_store: SecretStore::new().await?,
        })
    }

    pub async fn create(&mut self, profile: Profile) -> Result<ProfileId> {
        let id = profile.id.clone();
        self.profiles.insert(id.clone(), profile);
        // TODO: Persist to disk
        Ok(id)
    }

    pub async fn get(&self, id: &ProfileId) -> Result<Option<Profile>> {
        Ok(self.profiles.get(id).cloned())
    }

    pub async fn delete(&mut self, id: &ProfileId) -> Result<()> {
        self.profiles.remove(id);
        // TODO: Remove from disk
        Ok(())
    }

    pub async fn list(&self) -> Vec<ProfileId> {
        self.profiles.keys().cloned().collect()
    }

    pub async fn update(&mut self, profile: Profile) -> Result<()> {
        let id = profile.id.clone();
        self.profiles.insert(id, profile);
        // TODO: Update on disk
        Ok(())
    }
}
