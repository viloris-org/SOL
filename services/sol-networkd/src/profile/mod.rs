pub mod ethernet_profile;
pub mod vpn_profile;
pub mod wifi_profile;

use crate::security::SecretStore;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use tracing::warn;

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
        Self::new_at(
            PathBuf::from("/var/lib/sol-networkd/profiles"),
            SecretStore::new().await?,
        )
        .await
    }

    pub(crate) async fn new_at(storage_path: PathBuf, secret_store: SecretStore) -> Result<Self> {
        tokio::fs::create_dir_all(&storage_path)
            .await
            .with_context(|| format!("Failed to create {}", storage_path.display()))?;
        set_private_directory_permissions(&storage_path).await?;

        let mut store = Self {
            profiles: HashMap::new(),
            secret_store,
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
            if !entry.file_type().await?.is_file()
                || path.extension().and_then(|s| s.to_str()) != Some("json")
            {
                continue;
            }

            match self.load_profile(&path).await {
                Ok(profile) => {
                    let expected_name = format!("{}.json", profile.id.0);
                    if entry.file_name() != std::ffi::OsStr::new(&expected_name) {
                        warn!(
                            "Ignoring profile with mismatched file name: {}",
                            path.display()
                        );
                        continue;
                    }
                    self.profiles.insert(profile.id.clone(), profile);
                }
                Err(error) => {
                    warn!("Ignoring invalid profile {}: {error}", path.display());
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
        validate_profile_id(&profile.id)?;
        let filename = format!("{}.json", profile.id.0);
        let path = self.storage_path.join(filename);
        let temporary_path =
            self.storage_path
                .join(format!(".{}.{}.tmp", profile.id.0, uuid::Uuid::new_v4()));

        let data = serde_json::to_vec_pretty(profile)?;
        write_private_file(&temporary_path, &data).await?;
        if let Err(error) = tokio::fs::rename(&temporary_path, &path).await {
            let _ = tokio::fs::remove_file(&temporary_path).await;
            return Err(error).with_context(|| format!("Failed to replace {}", path.display()));
        }

        Ok(())
    }

    async fn remove_profile_file(&self, id: &ProfileId) -> Result<()> {
        validate_profile_id(id)?;
        let filename = format!("{}.json", id.0);
        let path = self.storage_path.join(filename);

        match tokio::fs::remove_file(path).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }

    pub async fn create(&mut self, profile: Profile) -> Result<ProfileId> {
        let id = profile.id.clone();
        validate_profile_id(&id)?;
        if self.profiles.contains_key(&id) {
            anyhow::bail!("Profile {id} already exists");
        }
        self.save_profile(&profile).await?;
        self.profiles.insert(id.clone(), profile);
        Ok(id)
    }

    pub async fn get(&self, id: &ProfileId) -> Result<Option<Profile>> {
        Ok(self.profiles.get(id).cloned())
    }

    pub async fn delete(&mut self, id: &ProfileId) -> Result<()> {
        if !self.profiles.contains_key(id) {
            anyhow::bail!("Profile {id} not found");
        }
        self.remove_profile_file(id).await?;
        self.profiles.remove(id);
        Ok(())
    }

    pub async fn list(&self) -> Vec<ProfileId> {
        let mut profiles = self.profiles.keys().cloned().collect::<Vec<_>>();
        profiles.sort_by(|left, right| left.0.cmp(&right.0));
        profiles
    }

    pub async fn update(&mut self, profile: Profile) -> Result<()> {
        let id = profile.id.clone();
        validate_profile_id(&id)?;
        if !self.profiles.contains_key(&id) {
            anyhow::bail!("Profile {id} not found");
        }
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

fn validate_profile_id(id: &ProfileId) -> Result<()> {
    let valid = !id.0.is_empty()
        && id.0.len() <= 128
        && id
            .0
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
    if !valid {
        anyhow::bail!("Invalid profile ID");
    }
    Ok(())
}

#[cfg(unix)]
async fn set_private_directory_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .await
        .with_context(|| format!("Failed to secure {}", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
async fn set_private_directory_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
async fn write_private_file(path: &Path, contents: &[u8]) -> Result<()> {
    use tokio::io::AsyncWriteExt;

    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .await?;
    file.write_all(contents).await?;
    file.sync_all().await?;
    Ok(())
}

#[cfg(not(unix))]
async fn write_private_file(path: &Path, contents: &[u8]) -> Result<()> {
    tokio::fs::write(path, contents).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::ethernet_profile::EthernetProfile;

    fn profile(id: &str, name: &str) -> Profile {
        Profile {
            id: ProfileId(id.into()),
            name: name.into(),
            profile_type: ProfileType::Ethernet(EthernetProfile::new_dhcp()),
            auto_connect: true,
            metered: false,
        }
    }

    async fn new_store(path: &Path) -> ProfileStore {
        ProfileStore::new_at(path.to_path_buf(), SecretStore::with_master_key([1; 32]))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn persists_updates_and_deletes_profiles() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = new_store(directory.path()).await;
        store.create(profile("profile-1", "Office")).await.unwrap();

        let mut updated = store
            .get(&ProfileId("profile-1".into()))
            .await
            .unwrap()
            .unwrap();
        updated.auto_connect = false;
        store.update(updated).await.unwrap();

        let reloaded = new_store(directory.path()).await;
        assert!(
            !reloaded
                .get(&ProfileId("profile-1".into()))
                .await
                .unwrap()
                .unwrap()
                .auto_connect
        );

        store.delete(&ProfileId("profile-1".into())).await.unwrap();
        assert!(store.list().await.is_empty());
        assert!(!directory.path().join("profile-1.json").exists());
    }

    #[tokio::test]
    async fn rejects_unsafe_and_duplicate_profile_ids() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = new_store(directory.path()).await;
        assert!(store.create(profile("../escape", "Unsafe")).await.is_err());

        store.create(profile("safe-id", "First")).await.unwrap();
        assert!(store.create(profile("safe-id", "Duplicate")).await.is_err());
    }

    #[tokio::test]
    async fn lists_profiles_deterministically() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = new_store(directory.path()).await;
        store.create(profile("z-profile", "Z")).await.unwrap();
        store.create(profile("a-profile", "A")).await.unwrap();
        assert_eq!(
            store.list().await,
            vec![ProfileId("a-profile".into()), ProfileId("z-profile".into())]
        );
    }
}
