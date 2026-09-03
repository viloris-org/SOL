//! Security rollback protection via hardware-backed monotonic index.
//!
//! # DEVELOPMENT SECURITY NOTICE
//!
//! **TPM integration is NOT yet implemented.** This is a Phase 7 gap.
//!
//! Without rollback protection:
//! ❌ Revoked deployments can still boot (security vulnerability)
//! ❌ Downgrade attacks are possible even with valid signatures
//!
//! Production requires:
//! - TPM 2.0 NV monotonic counter (`TPM2_NV_Increment`)
//! - Security version enforcement in deployment verification
//! - Irreversible advancement after successful promotion
//!
//! **DO NOT deploy to production** until this is implemented.
//!
//! Separates functional rollback (returning to an older but still trusted
//! deployment) from security rollback (returning to a revoked or security-old
//! deployment). See ADR-0026 Section 4.

use core::fmt;

/// Security rollback index interface.
///
/// Production implementations should use:
/// - TPM NV monotonic counter
/// - Platform-specific secure storage with monotonic properties
/// - Write-once fuses or RPMB (on embedded platforms)
pub trait RollbackProtection {
    /// Error type for rollback operations.
    type Error: fmt::Debug;

    /// Read current security epoch/version floor.
    ///
    /// # Errors
    ///
    /// Returns an error if the rollback index cannot be accessed or read.
    fn read_security_version(&mut self) -> Result<u32, Self::Error>;

    /// Advance security version (irreversible, monotonic only).
    ///
    /// This should only be called after a deployment is successfully promoted.
    /// Once advanced, all deployments with security_version < new_floor are
    /// rejected, even if their signatures remain cryptographically valid.
    ///
    /// # Errors
    ///
    /// Returns an error if the rollback index cannot be written or if
    /// advancing would violate monotonicity constraints.
    fn advance_security_version(&mut self, new_version: u32) -> Result<(), Self::Error>;
}

/// Security version enforcement policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecurityPolicy {
    /// Current security version floor (from rollback index).
    pub current_version: u32,

    /// Security version of the candidate deployment.
    pub candidate_version: u32,
}

impl SecurityPolicy {
    /// Check if candidate deployment meets security version requirements.
    pub const fn is_acceptable(&self) -> bool {
        self.candidate_version >= self.current_version
    }

    /// Check if this is a security-old deployment that must be rejected.
    pub const fn is_security_old(&self) -> bool {
        self.candidate_version < self.current_version
    }
}

/// TPM-backed rollback protection (production implementation).
#[cfg(feature = "tpm")]
pub struct TpmRollbackProtection {
    // TPM NV index for monotonic counter
    _marker: core::marker::PhantomData<()>,
}

#[cfg(feature = "tpm")]
impl TpmRollbackProtection {
    /// Create TPM-backed rollback protection.
    ///
    /// # Errors
    ///
    /// Returns an error if TPM initialization fails or the monotonic
    /// counter NV index cannot be accessed.
    pub fn new() -> Result<Self, TpmError> {
        // TODO: Initialize TPM interface
        // TODO: Locate or create NV monotonic counter
        unimplemented!("TPM integration pending")
    }
}

#[cfg(feature = "tpm")]
#[derive(Debug)]
pub enum TpmError {
    NotAvailable,
    NvIndexNotFound,
    AuthFailed,
    CommunicationError,
    MonotonicViolation,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn security_policy_rejects_old_versions() {
        let policy = SecurityPolicy {
            current_version: 10,
            candidate_version: 9,
        };
        assert!(!policy.is_acceptable());
        assert!(policy.is_security_old());
    }

    #[test]
    fn security_policy_accepts_current_and_newer() {
        let policy_same = SecurityPolicy {
            current_version: 10,
            candidate_version: 10,
        };
        assert!(policy_same.is_acceptable());
        assert!(!policy_same.is_security_old());

        let policy_newer = SecurityPolicy {
            current_version: 10,
            candidate_version: 11,
        };
        assert!(policy_newer.is_acceptable());
        assert!(!policy_newer.is_security_old());
    }
}
