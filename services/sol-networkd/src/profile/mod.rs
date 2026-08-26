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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: ProfileId,
    pub name: String,
    pub profile_type: ProfileType,
    pub auto_connect: bool,
    pub metered: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProfileType {
    WiFi(wifi_profile::WiFiProfile),
    Ethernet(ethernet_profile::EthernetProfile),
    Vpn(vpn_profile::VpnProfile),
}

/// Profile storage and management
pub struct ProfileStore {
    profiles: HashMap<ProfileId, Profile>,
    secret_store: SecretStore,
    storage_path: std::path::PathBuf,
}

impl ProfileStore {
    pub async fn new() -> Result<Self> {
        let storage_path = std::path::PathBuf::from("/var/lib/sol-networkd/profiles");

        // Create storage directory
        tokio::fs::create_dir_all(&storage_path).await.ok();

        let mut store = Self {
            profiles: HashMap::new(),
            secret_store: SecretStore::new().await?,
            storage_path,
        };

        // Load existing profiles from disk
        store.load_all().await?;

        Ok(store)
    }

    async fn load_all(&mut self) -> Result<()> {
        let mut entries = tokio::fs::read_dir(&self.storage_path).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(profile) = self.load_profile(&path).await {
                    self.profiles.insert(profile.id.clone(), profile);
                }
            }
        }

        Ok(())
    }

    async fn load_profile(&self, path: &std::path::Path) -> Result<Profile> {
        let data = tokio::fs::read(path).await?;
        let profile: Profile = serde_json::from_slice(&data)?;
        Ok(profile)
    }

    async fn save_profile(&self, profile: &Profile) -> Result<()> {
        let filename = format!("{}.json", profile.id.0);
        let path = self.storage_path.join(filename);

        let data = serde_json::to_vec_pretty(profile)?;
        tokio::fs::write(path, data).await?;

        Ok(())
    }

    async fn remove_profile_file(&self, id: &ProfileId) -> Result<()> {
        let filename = format!("{}.json", id.0);
        let path = self.storage_path.join(filename);

        tokio::fs::remove_file(path).await?;
        Ok(())
    }

    pub async fn create(&mut self, profile: Profile) -> Result<ProfileId> {
        let id = profile.id.clone();
        self.save_profile(&profile).await?;
        self.profiles.insert(id.clone(), profile);
        Ok(id)
    }

    pub async fn get(&self, id: &ProfileId) -> Result<Option<Profile>> {
        Ok(self.profiles.get(id).cloned())
    }

    pub async fn delete(&mut self, id: &ProfileId) -> Result<()> {
        self.profiles.remove(id);
        self.remove_profile_file(id).await.ok();
        Ok(())
    }

    pub async fn list(&self) -> Vec<ProfileId> {
        self.profiles.keys().cloned().collect()
    }

    pub async fn update(&mut self, profile: Profile) -> Result<()> {
        let id = profile.id.clone();
        self.save_profile(&profile).await?;
        self.profiles.insert(id, profile);
        Ok(())
    }

    pub async fn encrypt_passphrase(&self, passphrase: &str) -> Result<Vec<u8>> {
        self.secret_store.encrypt(passphrase.as_bytes()).await
    }

    pub async fn decrypt_passphrase(&self, encrypted: &[u8]) -> Result<Vec<u8>> {
        self.secret_store.decrypt(encrypted).await
    }
}
