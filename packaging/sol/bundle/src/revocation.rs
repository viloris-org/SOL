use serde::{Deserialize, Serialize};

use crate::error::{BundleError, Result};
use crate::{VerifiedSigner, fingerprint};

/// Repository-synchronized revocation cache.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevocationCache {
    /// Last successful sync as Unix epoch seconds UTC.
    pub last_sync: i64,
    /// Normal refresh interval, usually 24 hours.
    pub sync_interval_hours: u64,
    /// Revoked publisher keys.
    pub entries: Vec<RevocationEntry>,
}

/// One signature-covered repository revocation decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevocationEntry {
    /// `sha256:` public-key fingerprint.
    pub key_fingerprint: String,
    /// Signatures at or after this UTC epoch second are blocked.
    pub revoked_after: i64,
    /// Stable repository reason.
    pub reason: String,
    /// Optional safe replacement key fingerprint.
    pub safe_replacement: Option<String>,
}

/// Freshness classification from ADR-0029.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheState {
    /// Cache is younger than 24 hours.
    Fresh,
    /// Cache is 24–48 hours old.
    Stale,
    /// Cache is older than 48 hours.
    Expired,
    /// No cache was supplied.
    Missing,
}

impl RevocationCache {
    /// Classifies cache age without treating future clock skew as freshness.
    #[must_use]
    pub const fn state_at(&self, now: i64) -> CacheState {
        let age = now.saturating_sub(self.last_sync);
        if self.last_sync < 0 || now < self.last_sync {
            return CacheState::Expired;
        }
        match age / 3_600 {
            0..=23 => CacheState::Fresh,
            24..=47 => CacheState::Stale,
            _ => CacheState::Expired,
        }
    }

    /// Blocks a signer only when its signature is at/after the revocation cutoff.
    ///
    /// # Errors
    ///
    /// Returns [`BundleError::KeyRevoked`] when a matching effective entry exists.
    pub fn check_signer(&self, signer: &VerifiedSigner) -> Result<()> {
        let current = fingerprint(&signer.lineage.current_key);
        if let Some(entry) = self.entries.iter().find(|entry| {
            entry.key_fingerprint == current && signer.signed_at >= entry.revoked_after
        }) {
            return Err(BundleError::KeyRevoked {
                fingerprint: current,
                reason: entry.reason.clone(),
            });
        }
        Ok(())
    }
}
