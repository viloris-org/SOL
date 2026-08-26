use anyhow::{Result, Context};
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::rand::{SecureRandom, SystemRandom};
use ring::pbkdf2;
use std::num::NonZeroU32;
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
        let master_key = Self::derive_master_key().await?;

        Ok(Self {
            rng,
            master_key,
        })
    }

    async fn derive_master_key() -> Result<[u8; 32]> {
        // In production, this should:
        // 1. Read hardware-bound secret from TPM or /sys/class/dmi/id/product_uuid
        // 2. Combine with system salt from /var/lib/sol-networkd/salt
        // 3. Use PBKDF2 to derive key
        //
        // For now, we use a machine-specific identifier

        let machine_id = tokio::fs::read_to_string("/etc/machine-id")
            .await
            .context("Failed to read machine ID")?;

        let salt_path = "/var/lib/sol-networkd/salt";
        let salt = if let Ok(s) = tokio::fs::read(salt_path).await {
            s
        } else {
            // Generate new salt
            let mut new_salt = vec![0u8; 32];
            let rng = SystemRandom::new();
            rng.fill(&mut new_salt)
                .map_err(|_| anyhow::anyhow!("Failed to generate salt"))?;

            tokio::fs::create_dir_all("/var/lib/sol-networkd").await.ok();
            tokio::fs::write(salt_path, &new_salt).await.ok();
            new_salt
        };

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

    pub async fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let unbound_key = UnboundKey::new(&AES_256_GCM, &self.master_key)
            .map_err(|e| anyhow::anyhow!("Failed to create key: {:?}", e))?;
        let key = LessSafeKey::new(unbound_key);

        let mut nonce_bytes = [0u8; 12];
        self.rng.fill(&mut nonce_bytes)
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
        let decrypted = key.open_in_place(nonce, Aad::empty(), &mut in_out)
            .map_err(|e| anyhow::anyhow!("Decryption failed: {:?}", e))?;

        Ok(decrypted.to_vec())
    }
}
