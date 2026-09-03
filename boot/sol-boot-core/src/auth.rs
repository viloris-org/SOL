//! Authenticated boot state protocol with replay protection.
//!
//! Production boot state must be:
//! 1. Authenticated - only authorized entities can update it
//! 2. Replay-resistant - old states cannot be replayed
//! 3. Tamper-evident - corruption is detectable
//!
//! This module provides HMAC-SHA256 authentication and replay protection.
//! Software-based implementation is available for development/testing.
//! TPM-backed implementation (feature = "tpm") is for production use.

use core::fmt;
use hmac::{Hmac, Mac, KeyInit};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

/// Unpredictable attempt identity binding size.
pub const ATTEMPT_NONCE_SIZE: usize = 32;

/// HMAC tag size for state authentication.
pub const AUTH_TAG_SIZE: usize = 32;

/// Authenticated boot state format (v2).
///
/// This format replaces the development CRC32-only format with proper
/// authentication and replay protection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedBootState {
    /// Monotonic sequence number (prevents replay).
    pub sequence: u64,

    /// Deployment slot records.
    pub slot_a: Option<DeploymentRecord>,
    pub slot_b: Option<DeploymentRecord>,

    /// Unpredictable nonce for current attempt (if trial active).
    pub attempt_nonce: Option<[u8; ATTEMPT_NONCE_SIZE]>,

    /// HMAC-SHA256 tag over all fields above.
    pub auth_tag: [u8; AUTH_TAG_SIZE],
}

/// Authenticated deployment record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeploymentRecord {
    /// Content-addressed deployment identity (SHA-256 of manifest).
    pub content_id: [u8; 32],

    /// Deployment generation (monotonic).
    pub generation: u32,

    /// Priority for selection (higher = preferred).
    pub priority: u8,

    /// Remaining trial attempts.
    pub tries_remaining: u8,

    /// Bootable flag.
    pub bootable: bool,

    /// Successfully promoted flag.
    pub successful: bool,

    /// Security version (for rollback protection).
    pub security_version: u32,
}

/// Authenticated success report format (v2).
///
/// Reports must bind:
/// 1. Exact deployment identity (content-addressed)
/// 2. Unpredictable attempt nonce (prevents replay)
/// 3. Measured boot identity (TPM PCRs or equivalent)
/// 4. Staged checkpoint gates reached
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedSuccessReport {
    /// Content-addressed deployment identity.
    pub deployment_id: [u8; 32],

    /// Unpredictable attempt nonce from boot state.
    pub attempt_nonce: [u8; ATTEMPT_NONCE_SIZE],

    /// Measured boot identity (TPM PCR composite or equivalent).
    pub measured_boot_hash: [u8; 32],

    /// Checkpoint gates reached (bitfield).
    pub checkpoints: HealthCheckpoints,

    /// Timestamp (for audit, not security).
    pub timestamp_unix: u64,

    /// HMAC-SHA256 tag over all fields above.
    pub auth_tag: [u8; AUTH_TAG_SIZE],
}

/// Health checkpoint gates (bitfield).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HealthCheckpoints(pub u32);

impl HealthCheckpoints {
    /// UKI started successfully.
    pub const STARTED: Self = Self(1 << 0);

    /// dm-verity root mounted successfully.
    pub const ROOT_MOUNTED: Self = Self(1 << 1);

    /// Essential recovery/update services ready.
    pub const SERVICES_READY: Self = Self(1 << 2);

    /// Shared data compatibility verified.
    pub const DATA_COMPATIBLE: Self = Self(1 << 3);

    /// Promotion permitted (all gates passed).
    pub const PROMOTION_READY: Self = Self(1 << 4);

    /// Check if a specific checkpoint is reached.
    #[must_use]
    pub const fn has(self, checkpoint: Self) -> bool {
        (self.0 & checkpoint.0) != 0
    }

    /// Check if all checkpoints for promotion are reached.
    pub const fn ready_for_promotion(self) -> bool {
        self.has(Self::STARTED)
            && self.has(Self::ROOT_MOUNTED)
            && self.has(Self::SERVICES_READY)
            && self.has(Self::DATA_COMPATIBLE)
    }
}

/// Authenticated state storage interface.
///
/// Production implementations should use:
/// - TPM NV for replay-resistant storage
/// - Platform-specific secure storage with monotonic counters
/// - Hardware-backed key derivation for HMAC keys
pub trait AuthenticatedStorage {
    /// Error type for storage operations.
    type Error: core::fmt::Debug;

    /// Read current authenticated state.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage cannot be accessed or state is corrupted.
    fn read_state(&mut self) -> Result<Option<AuthenticatedBootState>, Self::Error>;

    /// Write new authenticated state (must increment sequence).
    ///
    /// # Errors
    ///
    /// Returns an error if the write fails or if sequence validation fails.
    fn write_state(&mut self, state: &AuthenticatedBootState) -> Result<(), Self::Error>;

    /// Read success report (consumed after read).
    ///
    /// # Errors
    ///
    /// Returns an error if the storage cannot be accessed.
    fn read_report(&mut self) -> Result<Option<AuthenticatedSuccessReport>, Self::Error>;

    /// Generate unpredictable attempt nonce.
    ///
    /// # Errors
    ///
    /// Returns an error if the random number generator fails.
    fn generate_nonce(&mut self) -> Result<[u8; ATTEMPT_NONCE_SIZE], Self::Error>;

    /// Compute HMAC-SHA256 authentication tag.
    #[must_use]
    fn compute_auth_tag(&self, message: &[u8]) -> [u8; AUTH_TAG_SIZE];

    /// Verify HMAC-SHA256 authentication tag.
    #[must_use]
    fn verify_auth_tag(&self, message: &[u8], tag: &[u8; AUTH_TAG_SIZE]) -> bool;
}

/// Software-based authenticated storage for development and testing.
///
/// **DEVELOPMENT ONLY** - Uses a software-derived HMAC key and in-memory
/// monotonic counter. This provides authentication and basic replay protection
/// but is NOT suitable for production use.
///
/// Production systems MUST use `TpmAuthenticatedStorage` with hardware-backed
/// monotonic counters and sealed keys.
pub struct SoftwareAuthenticatedStorage {
    /// HMAC key derived from a platform identifier or test seed.
    hmac_key: [u8; 32],

    /// In-memory monotonic sequence (resets on power loss - not production-safe).
    sequence_floor: u64,
}

impl SoftwareAuthenticatedStorage {
    /// Create software-based storage with a derived key.
    ///
    /// In development, the key can be derived from machine ID or a test seed.
    /// In production, this MUST NOT be used - use TPM-backed storage instead.
    ///
    /// # Security Warning
    ///
    /// This implementation:
    /// - ✅ Provides HMAC authentication (detects tampering)
    /// - ✅ Validates sequence monotonicity (detects replay within session)
    /// - ❌ Does NOT persist monotonic counter (replay possible after reboot)
    /// - ❌ Does NOT use hardware-protected keys
    /// - ❌ Does NOT bind to measured boot state
    #[must_use]
    pub fn new(hmac_key: [u8; 32]) -> Self {
        Self {
            hmac_key,
            sequence_floor: 0,
        }
    }

    /// Create storage for testing with a deterministic seed.
    #[cfg(test)]
    #[must_use]
    pub fn new_for_testing() -> Self {
        // Deterministic test key (DO NOT use in production)
        let test_key = Sha256::digest(b"SOL-BOOT-TEST-KEY-DO-NOT-USE-IN-PRODUCTION");
        Self::new(test_key.into())
    }

    /// Serialize state to authenticated bytes.
    ///
    /// Format: sequence (8) || slot_a || slot_b || nonce || HMAC-SHA256 (32)
    pub fn serialize_state(&self, state: &AuthenticatedBootState) -> [u8; AUTH_STATE_SIZE] {
        let mut buf = [0u8; AUTH_STATE_SIZE];
        let mut pos = 0;

        // Sequence number (8 bytes)
        buf[pos..pos + 8].copy_from_slice(&state.sequence.to_le_bytes());
        pos += 8;

        // Slot A (present flag + record if Some)
        if let Some(ref record) = state.slot_a {
            buf[pos] = 1;
            pos += 1;
            pos += self.serialize_deployment_record(record, &mut buf[pos..]);
        } else {
            buf[pos] = 0;
            pos += 1 + DEPLOYMENT_RECORD_SIZE;
        }

        // Slot B (present flag + record if Some)
        if let Some(ref record) = state.slot_b {
            buf[pos] = 1;
            pos += 1;
            pos += self.serialize_deployment_record(record, &mut buf[pos..]);
        } else {
            buf[pos] = 0;
            pos += 1 + DEPLOYMENT_RECORD_SIZE;
        }

        // Attempt nonce (present flag + nonce if Some)
        if let Some(ref nonce) = state.attempt_nonce {
            buf[pos] = 1;
            pos += 1;
            buf[pos..pos + ATTEMPT_NONCE_SIZE].copy_from_slice(nonce);
            pos += ATTEMPT_NONCE_SIZE;
        } else {
            buf[pos] = 0;
            pos += 1 + ATTEMPT_NONCE_SIZE;
        }

        // Compute HMAC over everything except the tag itself
        let tag = self.compute_auth_tag(&buf[..pos]);
        buf[pos..pos + AUTH_TAG_SIZE].copy_from_slice(&tag);

        buf
    }

    /// Serialize a deployment record into a buffer.
    fn serialize_deployment_record(&self, record: &DeploymentRecord, buf: &mut [u8]) -> usize {
        let mut pos = 0;
        buf[pos..pos + 32].copy_from_slice(&record.content_id);
        pos += 32;
        buf[pos..pos + 4].copy_from_slice(&record.generation.to_le_bytes());
        pos += 4;
        buf[pos] = record.priority;
        pos += 1;
        buf[pos] = record.tries_remaining;
        pos += 1;
        buf[pos] = u8::from(record.bootable);
        pos += 1;
        buf[pos] = u8::from(record.successful);
        pos += 1;
        buf[pos..pos + 4].copy_from_slice(&record.security_version.to_le_bytes());
        pos += 4;
        pos
    }

    /// Deserialize authenticated state from bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication fails, sequence is invalid, or format is corrupt.
    pub fn deserialize_state(&mut self, buf: &[u8; AUTH_STATE_SIZE]) -> Result<AuthenticatedBootState, AuthError> {
        // Extract and verify HMAC tag
        let message_len = AUTH_STATE_SIZE - AUTH_TAG_SIZE;
        let message = &buf[..message_len];
        let tag = &buf[message_len..];

        if !self.verify_auth_tag(message, tag.try_into().unwrap()) {
            return Err(AuthError::AuthenticationFailed);
        }

        // Parse fields
        let mut pos = 0;
        let sequence = u64::from_le_bytes(buf[pos..pos + 8].try_into().unwrap());
        pos += 8;

        // Validate sequence is monotonically increasing
        if sequence <= self.sequence_floor {
            return Err(AuthError::ReplayDetected {
                current: self.sequence_floor,
                attempted: sequence
            });
        }

        // Parse slot A
        let slot_a = if buf[pos] == 1 {
            pos += 1;
            let record = self.deserialize_deployment_record(&buf[pos..pos + DEPLOYMENT_RECORD_SIZE])?;
            pos += DEPLOYMENT_RECORD_SIZE;
            Some(record)
        } else {
            pos += 1 + DEPLOYMENT_RECORD_SIZE;
            None
        };

        // Parse slot B
        let slot_b = if buf[pos] == 1 {
            pos += 1;
            let record = self.deserialize_deployment_record(&buf[pos..pos + DEPLOYMENT_RECORD_SIZE])?;
            pos += DEPLOYMENT_RECORD_SIZE;
            Some(record)
        } else {
            pos += 1 + DEPLOYMENT_RECORD_SIZE;
            None
        };

        // Parse attempt nonce
        let attempt_nonce = if buf[pos] == 1 {
            pos += 1;
            let mut nonce = [0u8; ATTEMPT_NONCE_SIZE];
            nonce.copy_from_slice(&buf[pos..pos + ATTEMPT_NONCE_SIZE]);
            Some(nonce)
        } else {
            None
        };

        // Update sequence floor
        self.sequence_floor = sequence;

        Ok(AuthenticatedBootState {
            sequence,
            slot_a,
            slot_b,
            attempt_nonce,
            auth_tag: tag.try_into().unwrap(),
        })
    }

    /// Deserialize a deployment record from bytes.
    fn deserialize_deployment_record(&self, buf: &[u8]) -> Result<DeploymentRecord, AuthError> {
        if buf.len() < DEPLOYMENT_RECORD_SIZE {
            return Err(AuthError::InvalidFormat);
        }

        let mut pos = 0;
        let mut content_id = [0u8; 32];
        content_id.copy_from_slice(&buf[pos..pos + 32]);
        pos += 32;

        let generation = u32::from_le_bytes(buf[pos..pos + 4].try_into().unwrap());
        pos += 4;
        let priority = buf[pos];
        pos += 1;
        let tries_remaining = buf[pos];
        pos += 1;
        let bootable = buf[pos] != 0;
        pos += 1;
        let successful = buf[pos] != 0;
        pos += 1;
        let security_version = u32::from_le_bytes(buf[pos..pos + 4].try_into().unwrap());

        Ok(DeploymentRecord {
            content_id,
            generation,
            priority,
            tries_remaining,
            bootable,
            successful,
            security_version,
        })
    }

    /// Serialize success report to authenticated bytes.
    pub fn serialize_report(&self, report: &AuthenticatedSuccessReport) -> [u8; AUTH_REPORT_SIZE] {
        let mut buf = [0u8; AUTH_REPORT_SIZE];
        let mut pos = 0;

        buf[pos..pos + 32].copy_from_slice(&report.deployment_id);
        pos += 32;
        buf[pos..pos + 32].copy_from_slice(&report.attempt_nonce);
        pos += 32;
        buf[pos..pos + 32].copy_from_slice(&report.measured_boot_hash);
        pos += 32;
        buf[pos..pos + 4].copy_from_slice(&report.checkpoints.0.to_le_bytes());
        pos += 4;
        buf[pos..pos + 8].copy_from_slice(&report.timestamp_unix.to_le_bytes());
        pos += 8;

        let tag = self.compute_auth_tag(&buf[..pos]);
        buf[pos..pos + AUTH_TAG_SIZE].copy_from_slice(&tag);

        buf
    }

    /// Deserialize and verify success report from bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication fails or format is invalid.
    pub fn deserialize_report(&self, buf: &[u8; AUTH_REPORT_SIZE]) -> Result<AuthenticatedSuccessReport, AuthError> {
        let message_len = AUTH_REPORT_SIZE - AUTH_TAG_SIZE;
        let message = &buf[..message_len];
        let tag = &buf[message_len..];

        if !self.verify_auth_tag(message, tag.try_into().unwrap()) {
            return Err(AuthError::AuthenticationFailed);
        }

        let mut pos = 0;
        let mut deployment_id = [0u8; 32];
        deployment_id.copy_from_slice(&buf[pos..pos + 32]);
        pos += 32;

        let mut attempt_nonce = [0u8; ATTEMPT_NONCE_SIZE];
        attempt_nonce.copy_from_slice(&buf[pos..pos + 32]);
        pos += 32;

        let mut measured_boot_hash = [0u8; 32];
        measured_boot_hash.copy_from_slice(&buf[pos..pos + 32]);
        pos += 32;

        let checkpoints = HealthCheckpoints(u32::from_le_bytes(buf[pos..pos + 4].try_into().unwrap()));
        pos += 4;

        let timestamp_unix = u64::from_le_bytes(buf[pos..pos + 8].try_into().unwrap());

        Ok(AuthenticatedSuccessReport {
            deployment_id,
            attempt_nonce,
            measured_boot_hash,
            checkpoints,
            timestamp_unix,
            auth_tag: tag.try_into().unwrap(),
        })
    }
}

impl AuthenticatedStorage for SoftwareAuthenticatedStorage {
    type Error = AuthError;

    fn read_state(&mut self) -> Result<Option<AuthenticatedBootState>, Self::Error> {
        // In-memory implementation - no persistent storage
        // Adapter layer would handle actual file I/O
        Ok(None)
    }

    fn write_state(&mut self, state: &AuthenticatedBootState) -> Result<(), Self::Error> {
        // Validate sequence progression
        if state.sequence <= self.sequence_floor {
            return Err(AuthError::ReplayDetected {
                current: self.sequence_floor,
                attempted: state.sequence,
            });
        }

        // Verify authentication tag
        let serialized = self.serialize_state(state);
        let message_len = AUTH_STATE_SIZE - AUTH_TAG_SIZE;
        if !self.verify_auth_tag(&serialized[..message_len], &state.auth_tag) {
            return Err(AuthError::AuthenticationFailed);
        }

        self.sequence_floor = state.sequence;
        Ok(())
    }

    fn read_report(&mut self) -> Result<Option<AuthenticatedSuccessReport>, Self::Error> {
        // In-memory implementation - no persistent storage
        Ok(None)
    }

    fn generate_nonce(&mut self) -> Result<[u8; ATTEMPT_NONCE_SIZE], Self::Error> {
        let mut nonce = [0u8; ATTEMPT_NONCE_SIZE];

        // Derive nonce from sequence floor for deterministic testing
        // In production, the adapter would use RDRAND, EFI RNG protocol, or TPM RNG
        // Increment sequence_floor to ensure unique nonces
        self.sequence_floor += 1;
        let mut hasher = Sha256::new();
        hasher.update(b"software-nonce-");
        hasher.update(&self.sequence_floor.to_le_bytes());
        let seed = hasher.finalize();
        nonce.copy_from_slice(&seed[..ATTEMPT_NONCE_SIZE]);

        Ok(nonce)
    }

    fn compute_auth_tag(&self, message: &[u8]) -> [u8; AUTH_TAG_SIZE] {
        let mut mac = HmacSha256::new_from_slice(&self.hmac_key)
            .expect("HMAC key size is valid");
        mac.update(message);
        mac.finalize().into_bytes().into()
    }

    fn verify_auth_tag(&self, message: &[u8], tag: &[u8; AUTH_TAG_SIZE]) -> bool {
        let mut mac = HmacSha256::new_from_slice(&self.hmac_key)
            .expect("HMAC key size is valid");
        mac.update(message);
        mac.verify_slice(tag).is_ok()
    }
}

/// Authenticated state total size (including HMAC tag).
const DEPLOYMENT_RECORD_SIZE: usize = 44; // 32 + 4 + 1 + 1 + 1 + 1 + 4
const AUTH_STATE_SIZE: usize = 8 + (1 + DEPLOYMENT_RECORD_SIZE) * 2 + (1 + ATTEMPT_NONCE_SIZE) + AUTH_TAG_SIZE;
const AUTH_REPORT_SIZE: usize = 32 + 32 + 32 + 4 + 8 + AUTH_TAG_SIZE;

/// Authentication errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    /// HMAC authentication failed - state was tampered with.
    AuthenticationFailed,

    /// Replay attack detected - sequence number is not monotonically increasing.
    ReplayDetected { current: u64, attempted: u64 },

    /// Sequence number did not advance as required.
    SequenceViolation,

    /// Invalid format or corrupted data.
    InvalidFormat,

    /// Random number generation failed.
    RandomGenerationFailed,

    /// Nonce mismatch - success report does not match current attempt.
    NonceMismatch,
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthenticationFailed => write!(f, "HMAC authentication failed"),
            Self::ReplayDetected { current, attempted } => {
                write!(f, "replay detected: current={current}, attempted={attempted}")
            }
            Self::SequenceViolation => write!(f, "sequence number must advance"),
            Self::InvalidFormat => write!(f, "invalid format"),
            Self::RandomGenerationFailed => write!(f, "random generation failed"),
            Self::NonceMismatch => write!(f, "nonce mismatch"),
        }
    }
}

impl core::error::Error for AuthError {}

/// TPM-backed authenticated storage (production implementation).
///
/// Uses TPM NV indices for:
/// - Monotonic counter (prevents replay)
/// - HMAC key (sealed to PCRs)
/// - State storage (authenticated and replay-resistant)
#[cfg(feature = "tpm")]
pub struct TpmAuthenticatedStorage {
    // TPM interface will be implemented here
    _marker: core::marker::PhantomData<()>,
}

#[cfg(feature = "tpm")]
impl TpmAuthenticatedStorage {
    /// Create TPM-backed storage.
    ///
    /// # Errors
    ///
    /// Returns an error if TPM initialization fails or required NV indices
    /// cannot be accessed.
    pub fn new() -> Result<Self, TpmError> {
        // TODO: Initialize TPM interface
        // TODO: Locate or create NV indices
        // TODO: Unseal HMAC key or derive from PCRs
        unimplemented!("TPM integration pending - Phase 7")
    }
}

#[cfg(feature = "tpm")]
#[derive(Debug)]
pub enum TpmError {
    NotAvailable,
    NvIndexNotFound,
    AuthFailed,
    CommunicationError,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_gates_work() {
        let mut checks = HealthCheckpoints(0);
        assert!(!checks.ready_for_promotion());

        checks.0 |= HealthCheckpoints::STARTED.0;
        checks.0 |= HealthCheckpoints::ROOT_MOUNTED.0;
        checks.0 |= HealthCheckpoints::SERVICES_READY.0;
        checks.0 |= HealthCheckpoints::DATA_COMPATIBLE.0;

        assert!(checks.ready_for_promotion());
    }

    #[test]
    fn deployment_record_separation() {
        // Content ID is independent of physical slot
        let deployment_a = DeploymentRecord {
            content_id: [0x42; 32],
            generation: 10,
            priority: 100,
            tries_remaining: 0,
            bootable: true,
            successful: true,
            security_version: 5,
        };

        // Same content can be in multiple slots
        let deployment_b = deployment_a.clone();

        assert_eq!(deployment_a.content_id, deployment_b.content_id);
    }

    #[test]
    fn hmac_authentication_detects_tampering() {
        let storage = SoftwareAuthenticatedStorage::new_for_testing();

        let mut state = AuthenticatedBootState {
            sequence: 1,
            slot_a: Some(DeploymentRecord {
                content_id: [0x42; 32],
                generation: 10,
                priority: 100,
                tries_remaining: 3,
                bootable: true,
                successful: false,
                security_version: 5,
            }),
            slot_b: None,
            attempt_nonce: Some([0xAA; ATTEMPT_NONCE_SIZE]),
            auth_tag: [0; AUTH_TAG_SIZE],
        };

        // Compute valid auth tag
        let serialized = storage.serialize_state(&state);
        let message_len = AUTH_STATE_SIZE - AUTH_TAG_SIZE;
        state.auth_tag = storage.compute_auth_tag(&serialized[..message_len]);

        // Valid state should verify
        assert!(storage.verify_auth_tag(&serialized[..message_len], &state.auth_tag));

        // Tampered state should fail
        let mut tampered = serialized;
        tampered[0] ^= 0xFF; // Flip bits in sequence field
        assert!(!storage.verify_auth_tag(&tampered[..message_len], &state.auth_tag));
    }

    #[test]
    fn replay_protection_rejects_old_sequences() {
        let mut storage = SoftwareAuthenticatedStorage::new_for_testing();

        // Accept sequence 1
        let state1 = create_test_state(&storage, 1);
        storage.write_state(&state1).expect("sequence 1 should succeed");

        // Accept sequence 2
        let state2 = create_test_state(&storage, 2);
        storage.write_state(&state2).expect("sequence 2 should succeed");

        // Reject replayed sequence 1
        let err = storage.write_state(&state1).expect_err("replay should be rejected");
        assert!(matches!(err, AuthError::ReplayDetected { .. }));

        // Reject same sequence (also considered replay)
        let state2_dup = create_test_state(&storage, 2);
        let err = storage.write_state(&state2_dup).expect_err("duplicate sequence should be rejected");
        assert!(matches!(err, AuthError::ReplayDetected { .. }));
    }

    #[test]
    fn nonce_generation_produces_unique_values() {
        let mut storage = SoftwareAuthenticatedStorage::new_for_testing();

        let nonce1 = storage.generate_nonce().expect("nonce generation should succeed");
        let nonce2 = storage.generate_nonce().expect("nonce generation should succeed");

        // In test mode, nonces are deterministic but should differ across calls
        assert_ne!(nonce1, nonce2, "nonces should be unique");
    }

    #[test]
    fn success_report_authentication() {
        let storage = SoftwareAuthenticatedStorage::new_for_testing();

        let mut report = AuthenticatedSuccessReport {
            deployment_id: [0x42; 32],
            attempt_nonce: [0xAA; ATTEMPT_NONCE_SIZE],
            measured_boot_hash: [0xBB; 32],
            checkpoints: HealthCheckpoints(
                HealthCheckpoints::STARTED.0
                    | HealthCheckpoints::ROOT_MOUNTED.0
                    | HealthCheckpoints::SERVICES_READY.0
                    | HealthCheckpoints::DATA_COMPATIBLE.0,
            ),
            timestamp_unix: 1234567890,
            auth_tag: [0; AUTH_TAG_SIZE],
        };

        // Serialize with authentic tag
        let serialized = storage.serialize_report(&report);
        let message_len = AUTH_REPORT_SIZE - AUTH_TAG_SIZE;
        report.auth_tag.copy_from_slice(&serialized[message_len..]);

        // Should deserialize successfully
        let deserialized = storage.deserialize_report(&serialized).expect("valid report should deserialize");
        assert_eq!(deserialized.deployment_id, report.deployment_id);
        assert_eq!(deserialized.attempt_nonce, report.attempt_nonce);
        assert_eq!(deserialized.checkpoints.0, report.checkpoints.0);

        // Tampered report should fail
        let mut tampered = serialized;
        tampered[0] ^= 0xFF;
        let err = storage.deserialize_report(&tampered).expect_err("tampered report should fail");
        assert!(matches!(err, AuthError::AuthenticationFailed));
    }

    #[test]
    fn round_trip_state_serialization() {
        let mut storage = SoftwareAuthenticatedStorage::new_for_testing();

        let original = AuthenticatedBootState {
            sequence: 42,
            slot_a: Some(DeploymentRecord {
                content_id: [0x11; 32],
                generation: 100,
                priority: 200,
                tries_remaining: 3,
                bootable: true,
                successful: false,
                security_version: 10,
            }),
            slot_b: Some(DeploymentRecord {
                content_id: [0x22; 32],
                generation: 99,
                priority: 150,
                tries_remaining: 0,
                bootable: true,
                successful: true,
                security_version: 9,
            }),
            attempt_nonce: Some([0xCC; ATTEMPT_NONCE_SIZE]),
            auth_tag: [0; AUTH_TAG_SIZE],
        };

        // Serialize
        let serialized = storage.serialize_state(&original);

        // Deserialize
        let deserialized = storage.deserialize_state(&serialized).expect("valid state should deserialize");

        assert_eq!(deserialized.sequence, original.sequence);
        assert_eq!(deserialized.slot_a, original.slot_a);
        assert_eq!(deserialized.slot_b, original.slot_b);
        assert_eq!(deserialized.attempt_nonce, original.attempt_nonce);
    }

    #[test]
    fn empty_slots_serialize_correctly() {
        let storage = SoftwareAuthenticatedStorage::new_for_testing();

        let state = AuthenticatedBootState {
            sequence: 1,
            slot_a: None,
            slot_b: None,
            attempt_nonce: None,
            auth_tag: [0; AUTH_TAG_SIZE],
        };

        let serialized = storage.serialize_state(&state);
        let mut storage_mut = storage;
        let deserialized = storage_mut.deserialize_state(&serialized).expect("empty state should deserialize");

        assert_eq!(deserialized.slot_a, None);
        assert_eq!(deserialized.slot_b, None);
        assert_eq!(deserialized.attempt_nonce, None);
    }

    // Helper to create test state with valid authentication
    fn create_test_state(storage: &SoftwareAuthenticatedStorage, sequence: u64) -> AuthenticatedBootState {
        let mut state = AuthenticatedBootState {
            sequence,
            slot_a: Some(DeploymentRecord {
                content_id: [0x42; 32],
                generation: 10,
                priority: 100,
                tries_remaining: 3,
                bootable: true,
                successful: false,
                security_version: 5,
            }),
            slot_b: None,
            attempt_nonce: None,
            auth_tag: [0; AUTH_TAG_SIZE],
        };

        let serialized = storage.serialize_state(&state);
        let message_len = AUTH_STATE_SIZE - AUTH_TAG_SIZE;
        state.auth_tag = storage.compute_auth_tag(&serialized[..message_len]);
        state
    }
}
