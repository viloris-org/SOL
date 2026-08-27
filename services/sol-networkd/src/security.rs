use anyhow::{Context, Result};
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::pbkdf2;
use ring::rand::{SecureRandom, SystemRandom};
use std::num::NonZeroU32;
use std::path::Path;
use tracing::info;

/// Secure credential storage with encryption
pub struct SecretStore {
    rng: SystemRandom,
    master_key: [u8; 32],
}

impl SecretStore {
    pub async fn new() -> Result<Self> {
        info!("Initializing secret store");

        let rng = SystemRandom::new();
        let master_key = Self::derive_master_key(
            Path::new("/etc/machine-id"),
            Path::new("/var/lib/sol-networkd/salt"),
        )
        .await?;

        Ok(Self { rng, master_key })
    }

    async fn derive_master_key(machine_id_path: &Path, salt_path: &Path) -> Result<[u8; 32]> {
        // In production, this should:
        // 1. Read hardware-bound secret from TPM or /sys/class/dmi/id/product_uuid
        // 2. Combine with system salt from /var/lib/sol-networkd/salt
        // 3. Use PBKDF2 to derive key
        //
        // For now, we use a machine-specific identifier

        let machine_id = tokio::fs::read_to_string(machine_id_path)
            .await
            .context("Failed to read machine ID")?;
        let machine_id = machine_id.trim();
        if machine_id.is_empty() {
            anyhow::bail!("Machine ID is empty");
        }

        let salt = match tokio::fs::read(salt_path).await {
            Ok(salt) => salt,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let mut new_salt = vec![0u8; 32];
                let rng = SystemRandom::new();
                rng.fill(&mut new_salt)
                    .map_err(|_| anyhow::anyhow!("Failed to generate salt"))?;

                let parent = salt_path
                    .parent()
                    .ok_or_else(|| anyhow::anyhow!("Salt path has no parent directory"))?;
                tokio::fs::create_dir_all(parent)
                    .await
                    .context("Failed to create secret-store directory")?;
                write_secret_file(salt_path, &new_salt).await?;
                new_salt
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Failed to read {}", salt_path.display()));
            }
        };
        if salt.len() < 16 {
            anyhow::bail!("Secret-store salt is too short");
        }

        let mut key = [0u8; 32];
        pbkdf2::derive(
            pbkdf2::PBKDF2_HMAC_SHA256,
            NonZeroU32::new(100_000).unwrap(),
            &salt,
            machine_id.as_bytes(),
            &mut key,
        );

        Ok(key)
    }

    #[cfg(test)]
    pub(crate) fn with_master_key(master_key: [u8; 32]) -> Self {
        Self {
            rng: SystemRandom::new(),
            master_key,
        }
    }

    pub async fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let unbound_key = UnboundKey::new(&AES_256_GCM, &self.master_key)
            .map_err(|e| anyhow::anyhow!("Failed to create key: {:?}", e))?;
        let key = LessSafeKey::new(unbound_key);

        let mut nonce_bytes = [0u8; 12];
        self.rng
            .fill(&mut nonce_bytes)
            .map_err(|e| anyhow::anyhow!("Failed to generate nonce: {:?}", e))?;
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);

        let mut in_out = plaintext.to_vec();
        key.seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
            .map_err(|e| anyhow::anyhow!("Encryption failed: {:?}", e))?;

        // Prepend nonce to ciphertext
        let mut result = nonce_bytes.to_vec();
        result.extend_from_slice(&in_out);

        Ok(result)
    }

    pub async fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        if ciphertext.len() < 12 {
            return Err(anyhow::anyhow!("Ciphertext too short"));
        }

        // Extract nonce
        let nonce_bytes: [u8; 12] = ciphertext[..12].try_into()?;
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);

        // Extract encrypted data
        let encrypted_data = &ciphertext[12..];

        let unbound_key = UnboundKey::new(&AES_256_GCM, &self.master_key)
            .map_err(|e| anyhow::anyhow!("Failed to create key: {:?}", e))?;
        let key = LessSafeKey::new(unbound_key);

        let mut in_out = encrypted_data.to_vec();
        let decrypted = key
            .open_in_place(nonce, Aad::empty(), &mut in_out)
            .map_err(|e| anyhow::anyhow!("Decryption failed: {:?}", e))?;

        Ok(decrypted.to_vec())
    }
}

#[cfg(unix)]
async fn write_secret_file(path: &Path, contents: &[u8]) -> Result<()> {
    use tokio::io::AsyncWriteExt;

    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .await
        .with_context(|| format!("Failed to create {}", path.display()))?;
    file.write_all(contents).await?;
    file.sync_all().await?;
    Ok(())
}

#[cfg(not(unix))]
async fn write_secret_file(path: &Path, contents: &[u8]) -> Result<()> {
    tokio::fs::write(path, contents).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn encrypts_and_authenticates_credentials() {
        let store = SecretStore::with_master_key([7; 32]);
        let encrypted = store
            .encrypt(b"correct horse battery staple")
            .await
            .unwrap();
        assert_ne!(encrypted, b"correct horse battery staple");
        assert_eq!(
            store.decrypt(&encrypted).await.unwrap(),
            b"correct horse battery staple"
        );

        let mut tampered = encrypted;
        *tampered.last_mut().unwrap() ^= 1;
        assert!(store.decrypt(&tampered).await.is_err());
    }

    #[tokio::test]
    async fn derives_a_stable_key_from_test_files() {
        let directory = tempfile::tempdir().unwrap();
        let machine_id = directory.path().join("machine-id");
        let salt = directory.path().join("salt");
        tokio::fs::write(&machine_id, "test-machine\n")
            .await
            .unwrap();

        let first = SecretStore::derive_master_key(&machine_id, &salt)
            .await
            .unwrap();
        let second = SecretStore::derive_master_key(&machine_id, &salt)
            .await
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(tokio::fs::read(&salt).await.unwrap().len(), 32);
    }
}
