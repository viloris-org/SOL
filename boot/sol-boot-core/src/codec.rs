//! Canonical fixed-size durable encodings.
//!
//! # DEVELOPMENT SECURITY NOTICE
//!
//! The current implementation uses **CRC32 for integrity checking only**.
//! This is explicitly a development placeholder and provides:
//!
//! ✅ Torn-write detection (accidental corruption)
//! ❌ NO authentication (vulnerable to tampering)
//! ❌ NO replay protection (old states can be restored)
//!
//! **DO NOT deploy to production** until authenticated state storage is
//! implemented (see `auth.rs` and ADR-0026 Section 5).
//!
//! Production requirements:
//! - HMAC-SHA256 authentication tags
//! - TPM NV monotonic counters for replay resistance
//! - Unpredictable attempt nonces
//!
//! Track: Phase 7 authenticated state implementation

use super::{
    AttemptId, BootState, BootSuccessReport, DeploymentId, DeploymentRecord, DeploymentSlot,
    DeploymentStatus, StateError,
};
use core::error::Error;
use core::fmt;

/// Initial durable boot-state envelope format.
pub const BOOT_STATE_FORMAT_V1: u16 = 1;
/// Exact byte length of a format-1 durable boot-state envelope.
pub const BOOT_STATE_V1_SIZE: usize = 84;
const BOOT_STATE_V1_LENGTH: u16 = 84;
/// Initial authenticated boot-success payload format.
pub const BOOT_SUCCESS_FORMAT_V1: u16 = 1;
/// Exact byte length of a format-1 boot-success payload.
pub const BOOT_SUCCESS_V1_SIZE: usize = 36;
const BOOT_SUCCESS_V1_LENGTH: u16 = 36;

const STATE_MAGIC: [u8; 8] = *b"SOLSTATE";
const REPORT_MAGIC: [u8; 8] = *b"SOLBREPT";
const STATE_CHECKSUM_OFFSET: usize = BOOT_STATE_V1_SIZE - 4;
const REPORT_CHECKSUM_OFFSET: usize = BOOT_SUCCESS_V1_SIZE - 4;

/// Sequenced durable envelope used by redundant state copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableBootState {
    sequence: u64,
    state: BootState,
}

/// Independently replaceable durable state copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurableStateCopy {
    /// Durable state copy A.
    A,
    /// Durable state copy B.
    B,
}

impl DurableStateCopy {
    /// Returns the other durable state copy.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }
}

/// Newest valid state selected from redundant durable copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectedDurableState {
    copy: DurableStateCopy,
    envelope: DurableBootState,
}

impl SelectedDurableState {
    /// Returns the physical state copy that won selection.
    #[must_use]
    pub const fn copy(self) -> DurableStateCopy {
        self.copy
    }

    /// Returns the decoded durable envelope.
    #[must_use]
    pub const fn envelope(self) -> DurableBootState {
        self.envelope
    }
}

/// Selects the highest-sequence valid state while tolerating one missing,
/// corrupt, or torn copy.
///
/// Equal sequences must contain identical state. A conflicting equal sequence
/// is rejected instead of choosing an arbitrary authority.
///
/// # Errors
///
/// Returns an error when neither copy is valid or equal sequences disagree.
pub fn select_redundant_state(
    copy_a: Option<&[u8]>,
    copy_b: Option<&[u8]>,
) -> Result<SelectedDurableState, CodecError> {
    let a = copy_a.and_then(|bytes| DurableBootState::from_canonical_bytes(bytes).ok());
    let b = copy_b.and_then(|bytes| DurableBootState::from_canonical_bytes(bytes).ok());
    match (a, b) {
        (Some(a), Some(b)) if a.sequence > b.sequence => Ok(SelectedDurableState {
            copy: DurableStateCopy::A,
            envelope: a,
        }),
        (Some(a), Some(b)) if b.sequence > a.sequence => Ok(SelectedDurableState {
            copy: DurableStateCopy::B,
            envelope: b,
        }),
        (Some(a), Some(b)) if a == b => Ok(SelectedDurableState {
            copy: DurableStateCopy::A,
            envelope: a,
        }),
        (Some(_), Some(_)) => Err(CodecError::ConflictingSequence),
        (Some(envelope), None) => Ok(SelectedDurableState {
            copy: DurableStateCopy::A,
            envelope,
        }),
        (None, Some(envelope)) => Ok(SelectedDurableState {
            copy: DurableStateCopy::B,
            envelope,
        }),
        (None, None) => Err(CodecError::NoValidStateCopies),
    }
}

impl DurableBootState {
    /// Wraps an initial valid policy state at durable sequence one.
    ///
    /// # Errors
    ///
    /// Returns an error if the state violates boot-policy invariants.
    pub fn new(state: BootState) -> Result<Self, CodecError> {
        state.validate().map_err(CodecError::InvalidState)?;
        Ok(Self { sequence: 1, state })
    }

    /// Produces the next sequenced envelope for a policy mutation.
    ///
    /// # Errors
    ///
    /// Rejects invalid state or an exhausted durable sequence counter.
    pub fn advance(self, state: BootState) -> Result<Self, CodecError> {
        state.validate().map_err(CodecError::InvalidState)?;
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or(CodecError::SequenceExhausted)?;
        Ok(Self { sequence, state })
    }

    /// Returns the durable monotonic sequence used to order redundant copies.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }

    /// Returns the contained policy state.
    #[must_use]
    pub const fn state(self) -> BootState {
        self.state
    }

    /// Encodes the one canonical format-1 byte representation.
    #[must_use]
    pub fn canonical_bytes(self) -> [u8; BOOT_STATE_V1_SIZE] {
        let mut bytes = [0_u8; BOOT_STATE_V1_SIZE];
        bytes[..8].copy_from_slice(&STATE_MAGIC);
        put_u16(&mut bytes[8..10], BOOT_STATE_FORMAT_V1);
        put_u16(&mut bytes[10..12], BOOT_STATE_V1_LENGTH);
        put_u64(&mut bytes[16..24], self.sequence);
        bytes[24] = encode_slot(self.state.preferred);
        put_u64(&mut bytes[32..40], self.state.next_attempt);
        encode_record(&mut bytes[40..60], self.state.record(DeploymentSlot::A));
        encode_record(&mut bytes[60..80], self.state.record(DeploymentSlot::B));
        let checksum = crc32fast::hash(&bytes[..STATE_CHECKSUM_OFFSET]);
        put_u32(&mut bytes[STATE_CHECKSUM_OFFSET..], checksum);
        bytes
    }

    /// Decodes only canonical, checksummed format-1 state bytes.
    ///
    /// # Errors
    ///
    /// Rejects unknown formats, incorrect lengths, non-zero reserved bytes,
    /// torn/corrupt writes, non-canonical records, and invalid policy state.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, CodecError> {
        validate_header(
            bytes,
            STATE_MAGIC,
            BOOT_STATE_FORMAT_V1,
            BOOT_STATE_V1_SIZE,
            STATE_CHECKSUM_OFFSET,
        )?;
        if bytes[12..16].iter().any(|byte| *byte != 0)
            || bytes[25..32].iter().any(|byte| *byte != 0)
        {
            return Err(CodecError::NonCanonical);
        }
        let sequence = read_u64(&bytes[16..24]);
        if sequence == 0 {
            return Err(CodecError::ZeroSequence);
        }
        let preferred = decode_slot(bytes[24])?;
        let next_attempt = read_u64(&bytes[32..40]);
        let slot_a = decode_record(&bytes[40..60], DeploymentSlot::A)?;
        let slot_b = decode_record(&bytes[60..80], DeploymentSlot::B)?;
        let state = BootState {
            slots: [slot_a, slot_b],
            preferred,
            next_attempt,
        };
        state.validate().map_err(CodecError::InvalidState)?;
        let envelope = Self { sequence, state };
        if envelope.canonical_bytes().as_slice() != bytes {
            return Err(CodecError::NonCanonical);
        }
        Ok(envelope)
    }
}

impl BootSuccessReport {
    /// Encodes the canonical payload that an adapter must authenticate before
    /// applying it to durable state.
    #[must_use]
    pub fn canonical_bytes(self) -> [u8; BOOT_SUCCESS_V1_SIZE] {
        let mut bytes = [0_u8; BOOT_SUCCESS_V1_SIZE];
        bytes[..8].copy_from_slice(&REPORT_MAGIC);
        put_u16(&mut bytes[8..10], BOOT_SUCCESS_FORMAT_V1);
        put_u16(&mut bytes[10..12], BOOT_SUCCESS_V1_LENGTH);
        bytes[12] = encode_slot(self.deployment.slot);
        put_u64(&mut bytes[16..24], self.deployment.generation);
        put_u64(&mut bytes[24..32], self.attempt.0);
        let checksum = crc32fast::hash(&bytes[..REPORT_CHECKSUM_OFFSET]);
        put_u32(&mut bytes[REPORT_CHECKSUM_OFFSET..], checksum);
        bytes
    }

    /// Decodes a canonical success payload after transport authentication.
    ///
    /// This validates encoding and tear detection only. The caller must
    /// authenticate the bytes before passing the result to boot policy.
    ///
    /// # Errors
    ///
    /// Rejects malformed, corrupt, unsupported, or non-canonical payloads.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, CodecError> {
        validate_header(
            bytes,
            REPORT_MAGIC,
            BOOT_SUCCESS_FORMAT_V1,
            BOOT_SUCCESS_V1_SIZE,
            REPORT_CHECKSUM_OFFSET,
        )?;
        if bytes[13..16].iter().any(|byte| *byte != 0) {
            return Err(CodecError::NonCanonical);
        }
        let deployment = DeploymentId::new(decode_slot(bytes[12])?, read_u64(&bytes[16..24]))
            .map_err(CodecError::InvalidState)?;
        let attempt = AttemptId::new(read_u64(&bytes[24..32])).map_err(CodecError::InvalidState)?;
        let report = Self {
            deployment,
            attempt,
        };
        if report.canonical_bytes().as_slice() != bytes {
            return Err(CodecError::NonCanonical);
        }
        Ok(report)
    }
}

fn encode_record(output: &mut [u8], record: Option<DeploymentRecord>) {
    let Some(record) = record else {
        return;
    };
    output[0] = 1;
    put_u64(&mut output[4..12], record.id.generation);
    match record.status {
        DeploymentStatus::KnownGood => output[1] = 1,
        DeploymentStatus::Trial {
            remaining_attempts,
            pending_attempt,
        } => {
            output[1] = 2;
            output[2] = remaining_attempts;
            if let Some(attempt) = pending_attempt {
                output[3] = 1;
                put_u64(&mut output[12..20], attempt.0);
            }
        }
    }
}

fn decode_record(
    input: &[u8],
    slot: DeploymentSlot,
) -> Result<Option<DeploymentRecord>, CodecError> {
    match input[0] {
        0 => {
            if input.iter().all(|byte| *byte == 0) {
                Ok(None)
            } else {
                Err(CodecError::NonCanonical)
            }
        }
        1 => {
            let id = DeploymentId::new(slot, read_u64(&input[4..12]))
                .map_err(CodecError::InvalidState)?;
            let status = match input[1] {
                1 => {
                    if input[2] != 0 || input[3] != 0 || read_u64(&input[12..20]) != 0 {
                        return Err(CodecError::NonCanonical);
                    }
                    DeploymentStatus::KnownGood
                }
                2 => {
                    let pending_attempt = match input[3] {
                        0 => {
                            if read_u64(&input[12..20]) != 0 {
                                return Err(CodecError::NonCanonical);
                            }
                            None
                        }
                        1 => Some(
                            AttemptId::new(read_u64(&input[12..20]))
                                .map_err(CodecError::InvalidState)?,
                        ),
                        _ => return Err(CodecError::NonCanonical),
                    };
                    DeploymentStatus::Trial {
                        remaining_attempts: input[2],
                        pending_attempt,
                    }
                }
                _ => return Err(CodecError::NonCanonical),
            };
            Ok(Some(DeploymentRecord { id, status }))
        }
        _ => Err(CodecError::NonCanonical),
    }
}

fn validate_header(
    bytes: &[u8],
    magic: [u8; 8],
    supported_version: u16,
    expected_length: usize,
    checksum_offset: usize,
) -> Result<(), CodecError> {
    if bytes.len() != expected_length {
        return Err(CodecError::InvalidLength {
            expected: expected_length,
            actual: bytes.len(),
        });
    }
    if bytes[..8] != magic {
        return Err(CodecError::InvalidMagic);
    }
    let version = read_u16(&bytes[8..10]);
    if version != supported_version {
        return Err(CodecError::UnsupportedVersion(version));
    }
    if usize::from(read_u16(&bytes[10..12])) != expected_length {
        return Err(CodecError::InvalidLengthField);
    }
    if crc32fast::hash(&bytes[..checksum_offset]) != read_u32(&bytes[checksum_offset..]) {
        return Err(CodecError::ChecksumMismatch);
    }
    Ok(())
}

const fn encode_slot(slot: DeploymentSlot) -> u8 {
    match slot {
        DeploymentSlot::A => 0,
        DeploymentSlot::B => 1,
    }
}

const fn decode_slot(value: u8) -> Result<DeploymentSlot, CodecError> {
    match value {
        0 => Ok(DeploymentSlot::A),
        1 => Ok(DeploymentSlot::B),
        _ => Err(CodecError::NonCanonical),
    }
}

const fn put_u16(output: &mut [u8], value: u16) {
    output.copy_from_slice(&value.to_le_bytes());
}

const fn put_u32(output: &mut [u8], value: u32) {
    output.copy_from_slice(&value.to_le_bytes());
}

const fn put_u64(output: &mut [u8], value: u64) {
    output.copy_from_slice(&value.to_le_bytes());
}

const fn read_u16(input: &[u8]) -> u16 {
    u16::from_le_bytes([input[0], input[1]])
}

const fn read_u32(input: &[u8]) -> u32 {
    u32::from_le_bytes([input[0], input[1], input[2], input[3]])
}

const fn read_u64(input: &[u8]) -> u64 {
    u64::from_le_bytes([
        input[0], input[1], input[2], input[3], input[4], input[5], input[6], input[7],
    ])
}

/// Durable encoding or envelope sequencing failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecError {
    /// The input slice has the wrong physical byte length.
    InvalidLength {
        /// Required length for the selected format.
        expected: usize,
        /// Supplied length.
        actual: usize,
    },
    /// The record type magic is not recognized.
    InvalidMagic,
    /// The format version is not supported by this build.
    UnsupportedVersion(u16),
    /// The embedded fixed-length field is not canonical.
    InvalidLengthField,
    /// CRC32 tear/corruption detection failed.
    ChecksumMismatch,
    /// Reserved bytes, tags, or redundant values are non-canonical.
    NonCanonical,
    /// The decoded state violates boot-policy invariants.
    InvalidState(StateError),
    /// Durable envelope sequences start at one.
    ZeroSequence,
    /// The durable sequence counter cannot be advanced further.
    SequenceExhausted,
    /// Neither redundant state copy decoded successfully.
    NoValidStateCopies,
    /// Equal durable sequence numbers contained different valid states.
    ConflictingSequence,
}

impl fmt::Display for CodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength { expected, actual } => {
                write!(formatter, "expected {expected} bytes, got {actual}")
            }
            Self::InvalidMagic => formatter.write_str("unrecognized durable record magic"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported durable record version {version}")
            }
            Self::InvalidLengthField => {
                formatter.write_str("durable record length field is not canonical")
            }
            Self::ChecksumMismatch => formatter.write_str("durable record checksum mismatch"),
            Self::NonCanonical => formatter.write_str("durable record is not canonical"),
            Self::InvalidState(error) => write!(formatter, "invalid durable state: {error}"),
            Self::ZeroSequence => formatter.write_str("durable sequence must be greater than zero"),
            Self::SequenceExhausted => formatter.write_str("durable sequence is exhausted"),
            Self::NoValidStateCopies => {
                formatter.write_str("no valid redundant boot-state copy remains")
            }
            Self::ConflictingSequence => {
                formatter.write_str("equal durable sequences contain conflicting state")
            }
        }
    }
}

impl Error for CodecError {}
