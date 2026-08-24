//! Canonical signed deployment identity consumed by the UEFI adapter.

use super::{DeploymentId, DeploymentSlot, StateError};
use core::error::Error;
use core::fmt;

/// Initial signed deployment-descriptor format.
pub const DEPLOYMENT_FORMAT_V1: u16 = 1;
/// Bytes covered by the detached Ed25519 signature.
pub const DEPLOYMENT_V1_PAYLOAD_SIZE: usize = 104;
/// Payload plus a 64-byte Ed25519 signature.
pub const DEPLOYMENT_SIGNED_V1_SIZE: usize = DEPLOYMENT_V1_PAYLOAD_SIZE + 64;

const DEPLOYMENT_MAGIC: [u8; 8] = *b"SOLDEPLO";
const DEPLOYMENT_V1_LENGTH: u16 = 168;

/// Hash and length of a file bound by the signed deployment identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtifactBinding {
    length: u64,
    sha256: [u8; 32],
}

impl ArtifactBinding {
    /// Creates a complete-file binding.
    #[must_use]
    pub const fn new(length: u64, sha256: [u8; 32]) -> Self {
        Self { length, sha256 }
    }

    /// Returns the exact file length.
    #[must_use]
    pub const fn length(self) -> u64 {
        self.length
    }

    /// Returns the SHA-256 digest.
    #[must_use]
    pub const fn sha256(self) -> [u8; 32] {
        self.sha256
    }
}

/// Canonical deployment identity authorized by the SOL release key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeploymentDescriptor {
    deployment: DeploymentId,
    manifest: ArtifactBinding,
    uki: ArtifactBinding,
}

impl DeploymentDescriptor {
    /// Creates a descriptor for one conventional physical slot.
    #[must_use]
    pub const fn new(
        deployment: DeploymentId,
        manifest: ArtifactBinding,
        uki: ArtifactBinding,
    ) -> Self {
        Self {
            deployment,
            manifest,
            uki,
        }
    }

    /// Returns the exact slot and generation.
    #[must_use]
    pub const fn deployment(self) -> DeploymentId {
        self.deployment
    }

    /// Returns the canonical manifest binding.
    #[must_use]
    pub const fn manifest(self) -> ArtifactBinding {
        self.manifest
    }

    /// Returns the slot-specific UKI binding.
    #[must_use]
    pub const fn uki(self) -> ArtifactBinding {
        self.uki
    }

    /// Encodes exactly the bytes that must be signed.
    #[must_use]
    pub fn canonical_payload(self) -> [u8; DEPLOYMENT_V1_PAYLOAD_SIZE] {
        let mut bytes = [0_u8; DEPLOYMENT_V1_PAYLOAD_SIZE];
        bytes[..8].copy_from_slice(&DEPLOYMENT_MAGIC);
        bytes[8..10].copy_from_slice(&DEPLOYMENT_FORMAT_V1.to_le_bytes());
        bytes[10..12].copy_from_slice(&DEPLOYMENT_V1_LENGTH.to_le_bytes());
        bytes[12] = match self.deployment.slot() {
            DeploymentSlot::A => 0,
            DeploymentSlot::B => 1,
        };
        // Byte 13 is architecture: format 1 supports x86_64 only.
        bytes[13] = 1;
        bytes[16..24].copy_from_slice(&self.deployment.generation().to_le_bytes());
        bytes[24..32].copy_from_slice(&self.manifest.length.to_le_bytes());
        bytes[32..40].copy_from_slice(&self.uki.length.to_le_bytes());
        bytes[40..72].copy_from_slice(&self.manifest.sha256);
        bytes[72..104].copy_from_slice(&self.uki.sha256);
        bytes
    }

    /// Decodes only the canonical format-1 signed payload.
    ///
    /// # Errors
    ///
    /// Rejects incorrect lengths, magic, format, architecture, slot, reserved
    /// bytes, or deployment identity.
    pub fn from_canonical_payload(bytes: &[u8]) -> Result<Self, DeploymentDescriptorError> {
        if bytes.len() != DEPLOYMENT_V1_PAYLOAD_SIZE {
            return Err(DeploymentDescriptorError::InvalidLength);
        }
        if bytes[..8] != DEPLOYMENT_MAGIC {
            return Err(DeploymentDescriptorError::InvalidMagic);
        }
        let version = u16::from_le_bytes([bytes[8], bytes[9]]);
        if version != DEPLOYMENT_FORMAT_V1 {
            return Err(DeploymentDescriptorError::UnsupportedVersion(version));
        }
        if u16::from_le_bytes([bytes[10], bytes[11]]) != DEPLOYMENT_V1_LENGTH {
            return Err(DeploymentDescriptorError::InvalidLength);
        }
        if bytes[13] != 1 || bytes[14..16].iter().any(|byte| *byte != 0) {
            return Err(DeploymentDescriptorError::NonCanonical);
        }
        let slot = match bytes[12] {
            0 => DeploymentSlot::A,
            1 => DeploymentSlot::B,
            _ => return Err(DeploymentDescriptorError::NonCanonical),
        };
        let generation = read_u64(&bytes[16..24]);
        let deployment = DeploymentId::new(slot, generation)
            .map_err(DeploymentDescriptorError::InvalidIdentity)?;
        let descriptor = Self {
            deployment,
            manifest: ArtifactBinding::new(
                read_u64(&bytes[24..32]),
                bytes[40..72]
                    .try_into()
                    .map_err(|_| DeploymentDescriptorError::InvalidLength)?,
            ),
            uki: ArtifactBinding::new(
                read_u64(&bytes[32..40]),
                bytes[72..104]
                    .try_into()
                    .map_err(|_| DeploymentDescriptorError::InvalidLength)?,
            ),
        };
        if descriptor.canonical_payload().as_slice() != bytes {
            return Err(DeploymentDescriptorError::NonCanonical);
        }
        Ok(descriptor)
    }
}

/// A canonical descriptor plus its detached Ed25519 signature bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignedDeploymentDescriptor {
    descriptor: DeploymentDescriptor,
    signature: [u8; 64],
}

impl SignedDeploymentDescriptor {
    /// Joins an unsigned descriptor with a signature over its canonical payload.
    #[must_use]
    pub const fn new(descriptor: DeploymentDescriptor, signature: [u8; 64]) -> Self {
        Self {
            descriptor,
            signature,
        }
    }

    /// Returns the parsed descriptor.
    #[must_use]
    pub const fn descriptor(self) -> DeploymentDescriptor {
        self.descriptor
    }

    /// Returns the signature bytes for verification by the firmware adapter.
    #[must_use]
    pub const fn signature(self) -> [u8; 64] {
        self.signature
    }

    /// Encodes the signed record exactly as stored on the ESP.
    #[must_use]
    pub fn canonical_bytes(self) -> [u8; DEPLOYMENT_SIGNED_V1_SIZE] {
        let mut bytes = [0_u8; DEPLOYMENT_SIGNED_V1_SIZE];
        bytes[..DEPLOYMENT_V1_PAYLOAD_SIZE].copy_from_slice(&self.descriptor.canonical_payload());
        bytes[DEPLOYMENT_V1_PAYLOAD_SIZE..].copy_from_slice(&self.signature);
        bytes
    }

    /// Decodes a canonical signed record without authenticating it.
    ///
    /// # Errors
    ///
    /// Rejects records that are not the exact canonical format-1 size and
    /// encoding.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, DeploymentDescriptorError> {
        if bytes.len() != DEPLOYMENT_SIGNED_V1_SIZE {
            return Err(DeploymentDescriptorError::InvalidLength);
        }
        let descriptor =
            DeploymentDescriptor::from_canonical_payload(&bytes[..DEPLOYMENT_V1_PAYLOAD_SIZE])?;
        let signature = bytes[DEPLOYMENT_V1_PAYLOAD_SIZE..]
            .try_into()
            .map_err(|_| DeploymentDescriptorError::InvalidLength)?;
        Ok(Self::new(descriptor, signature))
    }
}

const fn read_u64(input: &[u8]) -> u64 {
    u64::from_le_bytes([
        input[0], input[1], input[2], input[3], input[4], input[5], input[6], input[7],
    ])
}

/// Malformed or unsupported signed deployment metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeploymentDescriptorError {
    /// The physical record or embedded length is wrong.
    InvalidLength,
    /// The magic does not identify a SOL deployment descriptor.
    InvalidMagic,
    /// The format is newer or otherwise unsupported.
    UnsupportedVersion(u16),
    /// Reserved values, architecture, or slot are not canonical.
    NonCanonical,
    /// The deployment identity violates policy invariants.
    InvalidIdentity(StateError),
}

impl fmt::Display for DeploymentDescriptorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength => formatter.write_str("invalid signed deployment length"),
            Self::InvalidMagic => formatter.write_str("invalid signed deployment magic"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported signed deployment version {version}")
            }
            Self::NonCanonical => formatter.write_str("signed deployment is not canonical"),
            Self::InvalidIdentity(error) => {
                write!(formatter, "invalid deployment identity: {error}")
            }
        }
    }
}

impl Error for DeploymentDescriptorError {}
