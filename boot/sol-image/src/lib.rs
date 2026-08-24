//! Reproducible, slot-bound SOL system deployment manifests.
//!
//! A deployment manifest binds the complete bootable unit selected by
//! `sol-boot`: kernel, initrd, read-only root image, runtime contracts, slot,
//! and generation. The schema deliberately contains no timestamp, source path,
//! host name, or other build-host state, so equal inputs produce equal bytes.
//!
//! This crate does not sign manifests or select the final system-image or UEFI
//! encoding. Those remain separate Phase 7 trust-policy decisions.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Current development deployment-manifest schema.
pub const MANIFEST_FORMAT_V1: u32 = 1;
/// UKI-aware deployment-manifest schema (M7.1).
pub const MANIFEST_FORMAT_V2: u32 = 2;

/// Supported manifest schema versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ManifestFormat {
    /// Original schema: kernel/initrd/root-image SHA-256 + slot/generation/version + runtime descriptors.
    V1,
    /// UKI-aware schema: adds UKI digest/length, component identities, dm-verity root hash, and slot-specific root identity.
    V2,
}

impl ManifestFormat {
    /// Returns the numeric schema identifier.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        match self {
            Self::V1 => MANIFEST_FORMAT_V1,
            Self::V2 => MANIFEST_FORMAT_V2,
        }
    }

    /// Parses a numeric format identifier.
    #[must_use]
    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            MANIFEST_FORMAT_V1 => Some(Self::V1),
            MANIFEST_FORMAT_V2 => Some(Self::V2),
            _ => None,
        }
    }
}

/// Original development deployment-manifest schema.
///
/// Kept as a compatibility alias for callers that predate format 2.
pub const MANIFEST_FORMAT: u32 = MANIFEST_FORMAT_V1;

/// Result returned by image-manifest operations.
pub type ImageResult<T> = Result<T, ImageError>;

/// A validation, encoding, or artifact I/O failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageError {
    /// A field did not satisfy the manifest schema.
    InvalidField { field: &'static str, reason: String },
    /// Two runtime declarations used the same runtime major.
    DuplicateRuntime(String),
    /// Input was valid JSON but not the one canonical byte representation.
    NonCanonicalManifest,
    /// A manifest uses a schema this implementation does not understand.
    UnsupportedFormat(u32),
    /// An artifact does not match its manifest binding.
    ArtifactMismatch(&'static str),
    /// Filesystem input or output failed.
    Io(String),
    /// JSON encoding or decoding failed.
    Encoding(String),
}

impl fmt::Display for ImageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidField { field, reason } => write!(formatter, "invalid {field}: {reason}"),
            Self::DuplicateRuntime(runtime) => {
                write!(formatter, "runtime {runtime} is declared more than once")
            }
            Self::NonCanonicalManifest => {
                formatter.write_str("manifest is not in canonical SOL encoding")
            }
            Self::UnsupportedFormat(format) => {
                write!(formatter, "unsupported deployment manifest format {format}")
            }
            Self::ArtifactMismatch(artifact) => {
                write!(formatter, "{artifact} does not match its manifest binding")
            }
            Self::Io(error) => write!(formatter, "artifact I/O failure: {error}"),
            Self::Encoding(error) => write!(formatter, "manifest encoding failure: {error}"),
        }
    }
}

impl Error for ImageError {}

/// Physical A/B system deployment slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeploymentSlot {
    /// Deployment slot A.
    A,
    /// Deployment slot B.
    B,
}

impl DeploymentSlot {
    /// Parses the user-facing slot label.
    ///
    /// # Errors
    ///
    /// Returns [`ImageError::InvalidField`] unless the value is `A` or `B`
    /// (case-insensitive).
    pub fn parse(value: &str) -> ImageResult<Self> {
        match value {
            "A" | "a" => Ok(Self::A),
            "B" | "b" => Ok(Self::B),
            _ => Err(ImageError::InvalidField {
                field: "slot",
                reason: "expected A or B".to_owned(),
            }),
        }
    }
}

impl fmt::Display for DeploymentSlot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::A => "A",
            Self::B => "B",
        })
    }
}

/// Supported system-image architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Architecture {
    /// Initial supported 64-bit x86 UEFI target.
    #[serde(rename = "x86_64")]
    X86_64,
}

/// A validated SHA-256 digest encoded as lowercase hexadecimal in a manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Sha256Digest([u8; 32]);

impl Sha256Digest {
    /// Parses exactly 32 digest bytes from lowercase hexadecimal.
    ///
    /// # Errors
    ///
    /// Returns [`ImageError::InvalidField`] when the digest is not canonical
    /// lowercase SHA-256 hexadecimal.
    pub fn parse(value: &str) -> ImageResult<Self> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(ImageError::InvalidField {
                field: "sha256",
                reason: "expected 64 lowercase hexadecimal characters".to_owned(),
            });
        }

        let decoded = hex::decode(value).map_err(|error| ImageError::InvalidField {
            field: "sha256",
            reason: error.to_string(),
        })?;
        let bytes: [u8; 32] = decoded.try_into().map_err(|_| ImageError::InvalidField {
            field: "sha256",
            reason: "expected a 32-byte digest".to_owned(),
        })?;
        Ok(Self(bytes))
    }

    fn from_reader(mut reader: impl Read) -> ImageResult<(Self, u64)> {
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 8 * 1024];
        let mut size = 0_u64;
        loop {
            let read = reader
                .read(&mut buffer)
                .map_err(|error| ImageError::Io(error.to_string()))?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            size = size
                .checked_add(read as u64)
                .ok_or_else(|| ImageError::InvalidField {
                    field: "artifact.size",
                    reason: "artifact exceeds the supported size".to_owned(),
                })?;
        }
        Ok((Self(hasher.finalize().into()), size))
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&hex::encode(self.0))
    }
}

impl Serialize for Sha256Digest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Sha256Digest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

/// Digest and byte length for one immutable deployment artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactBinding {
    sha256: Sha256Digest,
    size: u64,
}

impl ArtifactBinding {
    /// Computes a binding directly from an artifact file.
    ///
    /// # Errors
    ///
    /// Returns an error when the path cannot be read, is not a regular file,
    /// is empty, or exceeds the supported length.
    pub fn from_path(path: impl AsRef<Path>) -> ImageResult<Self> {
        let path = path.as_ref();
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| ImageError::Io(format!("{}: {error}", path.display())))?;
        if !metadata.file_type().is_file() {
            return Err(ImageError::InvalidField {
                field: "artifact",
                reason: format!("{} is not a regular file", path.display()),
            });
        }
        let file = File::open(path)
            .map_err(|error| ImageError::Io(format!("{}: {error}", path.display())))?;
        let (sha256, size) = Sha256Digest::from_reader(file)?;
        if size == 0 {
            return Err(ImageError::InvalidField {
                field: "artifact.size",
                reason: format!("{} is empty", path.display()),
            });
        }
        Ok(Self { sha256, size })
    }

    /// Returns the SHA-256 content digest.
    #[must_use]
    pub const fn sha256(&self) -> Sha256Digest {
        self.sha256
    }

    /// Returns the exact artifact length in bytes.
    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }

    fn validate(&self, field: &'static str) -> ImageResult<()> {
        if self.size == 0 {
            return Err(ImageError::InvalidField {
                field,
                reason: "artifact must not be empty".to_owned(),
            });
        }
        Ok(())
    }
}

/// A canonical stable identifier for a logical kernel or initrd component within
/// a deployed system image.
///
/// The identity is scoped to a slot and generation so the same component
/// digest can be independently present in both A and B slots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentIdentity {
    /// Slot-scoped component logical identifier, e.g. `kernel-x86_64-6.9`.
    name: String,
    /// Slot-specific component identity string, e.g.
    /// `slot-B-gen-42-root-abc123`.
    slot_identity: String,
}

impl ComponentIdentity {
    /// Creates a validated component identity.
    ///
    /// # Errors
    ///
    /// Returns [`ImageError::InvalidField`] when either identifier is empty or
    /// exceeds 256 characters or contains disallowed characters.
    pub fn new(name: impl Into<String>, slot_identity: impl Into<String>) -> ImageResult<Self> {
        let name = name.into();
        let slot_identity = slot_identity.into();
        validate_identifier(&name, "component.name")?;
        validate_identifier(&slot_identity, "component.slot_identity")?;
        Ok(Self {
            name,
            slot_identity,
        })
    }

    /// Returns the stable component name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the slot-specific identity string.
    #[must_use]
    pub fn slot_identity(&self) -> &str {
        &self.slot_identity
    }

    fn validate(&self) -> ImageResult<()> {
        validate_identifier(&self.name, "component.name")?;
        validate_identifier(&self.slot_identity, "component.slot_identity")
    }
}

/// Digest and byte length of the complete Unified Kernel Image (UKI) installed
/// in a slot.
///
/// The UKI contains the Linux EFI stub, kernel, initrd, immutable command line,
/// and release metadata. Its digest is independent of the individual kernel and
/// initrd component digests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UkiBinding {
    sha256: Sha256Digest,
    size: u64,
}

impl UkiBinding {
    /// Computes a UKI binding directly from a UKI file.
    ///
    /// # Errors
    ///
    /// Returns an error when the path cannot be read, is not a regular file,
    /// is empty, or exceeds the supported length.
    pub fn from_path(path: impl AsRef<Path>) -> ImageResult<Self> {
        let path = path.as_ref();
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| ImageError::Io(format!("{}: {error}", path.display())))?;
        if !metadata.file_type().is_file() {
            return Err(ImageError::InvalidField {
                field: "uki",
                reason: format!("{} is not a regular file", path.display()),
            });
        }
        let file = File::open(path)
            .map_err(|error| ImageError::Io(format!("{}: {error}", path.display())))?;
        let (sha256, size) = Sha256Digest::from_reader(file)?;
        if size == 0 {
            return Err(ImageError::InvalidField {
                field: "uki.size",
                reason: format!("{} is empty", path.display()),
            });
        }
        Ok(Self { sha256, size })
    }

    /// Returns the UKI SHA-256 digest.
    #[must_use]
    pub const fn sha256(&self) -> Sha256Digest {
        self.sha256
    }

    /// Returns the exact UKI length in bytes.
    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }

    fn validate(&self) -> ImageResult<()> {
        if self.size == 0 {
            return Err(ImageError::InvalidField {
                field: "uki.size",
                reason: "UKI must not be empty".to_owned(),
            });
        }
        Ok(())
    }
}

/// dm-verity root hash and slot-specific root identity for a booted system
/// deployment.
///
/// The root hash is the hash tree root used by the kernel's dm-verity driver to
/// authenticate every block of the read-only root image. The slot identity
/// distinguishes the same hash tree across A/B slots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DmVerityBinding {
    /// Hex-encoded SHA-256 root hash of the dm-verity hash tree.
    root_hash: String,
    /// Slot-specific root identity, e.g. `slot-B-root-abc123`.
    slot_root_identity: String,
}

impl DmVerityBinding {
    /// Creates a validated dm-verity binding.
    ///
    /// # Errors
    ///
    /// Returns [`ImageError::InvalidField`] when the root hash is not 64 lowercase
    /// hexadecimal characters or the slot identity is empty or too long.
    pub fn new(
        root_hash: impl Into<String>,
        slot_root_identity: impl Into<String>,
    ) -> ImageResult<Self> {
        let root_hash = root_hash.into();
        let slot_root_identity = slot_root_identity.into();
        if root_hash.len() != 64
            || !root_hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(ImageError::InvalidField {
                field: "dm_verity.root_hash",
                reason: "expected 64 lowercase hexadecimal characters".to_owned(),
            });
        }
        validate_identifier(&slot_root_identity, "dm_verity.slot_root_identity")?;
        Ok(Self {
            root_hash,
            slot_root_identity,
        })
    }

    /// Returns the hex-encoded dm-verity root hash.
    #[must_use]
    pub fn root_hash(&self) -> &str {
        &self.root_hash
    }

    /// Returns the slot-specific root identity.
    #[must_use]
    pub fn slot_root_identity(&self) -> &str {
        &self.slot_root_identity
    }

    fn validate(&self) -> ImageResult<()> {
        if self.root_hash.len() != 64
            || !self
                .root_hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(ImageError::InvalidField {
                field: "dm_verity.root_hash",
                reason: "expected 64 lowercase hexadecimal characters".to_owned(),
            });
        }
        validate_identifier(&self.slot_root_identity, "dm_verity.slot_root_identity")?;
        Ok(())
    }
}

/// UKI-aware fields added by deployment-manifest format 2.
///
/// Grouping these values ensures callers cannot accidentally construct a V2
/// manifest with only part of its UKI or dm-verity identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UkiDeploymentBinding {
    uki: UkiBinding,
    kernel_component: ComponentIdentity,
    initrd_component: ComponentIdentity,
    dm_verity: DmVerityBinding,
}

impl UkiDeploymentBinding {
    /// Binds the complete UKI file and its logical component/root identities.
    ///
    /// # Errors
    ///
    /// Returns an error when the UKI cannot be bound as a non-empty regular
    /// file or any supplied identity is invalid.
    pub fn from_path(
        uki_path: impl AsRef<Path>,
        kernel_component: ComponentIdentity,
        initrd_component: ComponentIdentity,
        dm_verity: DmVerityBinding,
    ) -> ImageResult<Self> {
        let binding = Self {
            uki: UkiBinding::from_path(uki_path)?,
            kernel_component,
            initrd_component,
            dm_verity,
        };
        binding.validate()?;
        Ok(binding)
    }

    fn validate(&self) -> ImageResult<()> {
        self.uki.validate()?;
        self.kernel_component.validate()?;
        self.initrd_component.validate()?;
        self.dm_verity.validate()
    }
}

fn validate_identifier(value: &str, field: &'static str) -> ImageResult<()> {
    if value.is_empty() || value.len() > 256 {
        return Err(ImageError::InvalidField {
            field,
            reason: format!("must be 1-256 characters, got {value:?}"),
        });
    }
    if !value.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
    }) {
        return Err(ImageError::InvalidField {
            field,
            reason: format!("{value:?} contains disallowed characters"),
        });
    }
    Ok(())
}

/// The three immutable artifacts that form a bootable system deployment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeploymentArtifacts {
    kernel: ArtifactBinding,
    initrd: ArtifactBinding,
    root_image: ArtifactBinding,
}

impl DeploymentArtifacts {
    /// Hashes the three artifacts and records their exact lengths.
    ///
    /// # Errors
    ///
    /// Returns an error when any path cannot be bound as a non-empty regular
    /// file.
    pub fn from_paths(paths: &ArtifactPaths) -> ImageResult<Self> {
        Ok(Self {
            kernel: ArtifactBinding::from_path(&paths.kernel)?,
            initrd: ArtifactBinding::from_path(&paths.initrd)?,
            root_image: ArtifactBinding::from_path(&paths.root_image)?,
        })
    }

    /// Returns the kernel binding.
    #[must_use]
    pub const fn kernel(&self) -> &ArtifactBinding {
        &self.kernel
    }

    /// Returns the initrd binding.
    #[must_use]
    pub const fn initrd(&self) -> &ArtifactBinding {
        &self.initrd
    }

    /// Returns the read-only root-image binding.
    #[must_use]
    pub const fn root_image(&self) -> &ArtifactBinding {
        &self.root_image
    }

    fn validate(&self) -> ImageResult<()> {
        self.kernel.validate("artifacts.kernel")?;
        self.initrd.validate("artifacts.initrd")?;
        self.root_image.validate("artifacts.root_image")
    }
}

/// Local paths used only while building or verifying a manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactPaths {
    /// Linux kernel or unified-kernel payload bound to the slot.
    pub kernel: PathBuf,
    /// Initrd bound to the same slot and generation.
    pub initrd: PathBuf,
    /// Immutable system root image.
    pub root_image: PathBuf,
}

/// One stable runtime contract exposed by this system deployment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeDescriptor {
    name: String,
    contract_revision: u64,
    features: Vec<String>,
}

impl RuntimeDescriptor {
    /// Creates a validated descriptor and canonicalizes feature ordering.
    ///
    /// # Errors
    ///
    /// Returns an error for a non-canonical runtime name, a zero contract
    /// revision, or an invalid feature identifier.
    pub fn new(
        name: impl Into<String>,
        contract_revision: u64,
        mut features: Vec<String>,
    ) -> ImageResult<Self> {
        let name = name.into();
        validate_runtime_name(&name)?;
        if contract_revision == 0 {
            return Err(ImageError::InvalidField {
                field: "runtime.contract_revision",
                reason: "revision must be greater than zero".to_owned(),
            });
        }
        for feature in &features {
            validate_feature(feature)?;
        }
        features.sort_unstable();
        features.dedup();
        Ok(Self {
            name,
            contract_revision,
            features,
        })
    }

    /// Returns a name such as `sol-runtime-1`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the monotonic contract revision within the runtime major.
    #[must_use]
    pub const fn contract_revision(&self) -> u64 {
        self.contract_revision
    }

    /// Returns the sorted, unique stable feature set.
    #[must_use]
    pub fn features(&self) -> &[String] {
        &self.features
    }

    fn validate(&self) -> ImageResult<u32> {
        let major = validate_runtime_name(&self.name)?;
        if self.contract_revision == 0 {
            return Err(ImageError::InvalidField {
                field: "runtime.contract_revision",
                reason: "revision must be greater than zero".to_owned(),
            });
        }
        if self.features.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(ImageError::InvalidField {
                field: "runtime.features",
                reason: "features must be sorted and unique".to_owned(),
            });
        }
        for feature in &self.features {
            validate_feature(feature)?;
        }
        Ok(major)
    }
}

/// Complete immutable system-deployment identity consumed by `sol-boot`.
///
/// The `format` field selects the schema version. Format 1 carries the
/// original kernel/initrd/root-image bindings. Format 2 additionally binds the
/// UKI digest/length, kernel/initrd component identities, and dm-verity root
/// hash. The two formats are not silently reinterpreted: a V1 manifest cannot
/// carry V2 fields and a V2 manifest must supply all V2 fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeploymentManifest {
    /// Schema version: 1 (original) or 2 (UKI-aware).
    format: u32,
    architecture: Architecture,
    slot: DeploymentSlot,
    generation: u64,
    system_version: String,
    artifacts: DeploymentArtifacts,
    runtimes: Vec<RuntimeDescriptor>,
    /// V2-only: complete UKI digest and byte length.
    #[serde(skip_serializing_if = "Option::is_none")]
    uki: Option<UkiBinding>,
    /// V2-only: logical identities for the kernel and initrd components used to
    /// compose the UKI.
    #[serde(skip_serializing_if = "Option::is_none")]
    kernel_component: Option<ComponentIdentity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    initrd_component: Option<ComponentIdentity>,
    /// V2-only: dm-verity root hash and slot-specific root identity.
    #[serde(skip_serializing_if = "Option::is_none")]
    dm_verity: Option<DmVerityBinding>,
}

impl DeploymentManifest {
    /// Format 1 constructor: builds a V1 manifest with the original artifact
    /// bindings and no UKI or dm-verity fields.
    ///
    /// # Errors
    ///
    /// Returns an error for a zero generation, invalid system version,
    /// invalid artifacts, no runtimes, or duplicate/invalid runtime majors.
    pub fn new(
        slot: DeploymentSlot,
        generation: u64,
        system_version: impl Into<String>,
        artifacts: DeploymentArtifacts,
        mut runtimes: Vec<RuntimeDescriptor>,
    ) -> ImageResult<Self> {
        let system_version = system_version.into();
        validate_system_version(&system_version)?;
        if generation == 0 {
            return Err(ImageError::InvalidField {
                field: "generation",
                reason: "generation must be greater than zero".to_owned(),
            });
        }
        artifacts.validate()?;
        for runtime in &runtimes {
            runtime.validate()?;
        }
        runtimes.sort_unstable_by_key(|runtime| runtime_major(&runtime.name));
        if let Some(duplicate) = runtimes
            .windows(2)
            .find(|pair| pair[0].name == pair[1].name)
        {
            return Err(ImageError::DuplicateRuntime(duplicate[0].name.clone()));
        }
        if runtimes.is_empty() {
            return Err(ImageError::InvalidField {
                field: "runtimes",
                reason: "at least one runtime descriptor is required".to_owned(),
            });
        }

        Ok(Self {
            format: MANIFEST_FORMAT_V1,
            architecture: Architecture::X86_64,
            slot,
            generation,
            system_version,
            artifacts,
            runtimes,
            uki: None,
            kernel_component: None,
            initrd_component: None,
            dm_verity: None,
        })
    }

    /// Format 2 constructor: builds a UKI-aware V2 manifest.
    ///
    /// All V2 fields are required. The `format` field is set automatically.
    ///
    /// # Errors
    ///
    /// Returns an error for the same reasons as [`Self::new`] plus any invalid
    /// V2 field.
    pub fn new_v2(
        slot: DeploymentSlot,
        generation: u64,
        system_version: impl Into<String>,
        artifacts: DeploymentArtifacts,
        runtimes: Vec<RuntimeDescriptor>,
        uki_deployment: UkiDeploymentBinding,
    ) -> ImageResult<Self> {
        uki_deployment.validate()?;
        let manifest = Self::new(slot, generation, system_version, artifacts, runtimes)?;
        Ok(Self {
            format: MANIFEST_FORMAT_V2,
            uki: Some(uki_deployment.uki),
            kernel_component: Some(uki_deployment.kernel_component),
            initrd_component: Some(uki_deployment.initrd_component),
            dm_verity: Some(uki_deployment.dm_verity),
            ..manifest
        })
    }

    /// Returns the manifest schema version.
    #[must_use]
    pub const fn format_version(&self) -> u32 {
        self.format
    }

    /// Returns the schema as a typed [`ManifestFormat`].
    #[must_use]
    pub const fn manifest_format(&self) -> Option<ManifestFormat> {
        ManifestFormat::from_u32(self.format)
    }

    /// Returns the UKI binding for V2 manifests, or `None` for V1.
    #[must_use]
    pub const fn uki(&self) -> Option<&UkiBinding> {
        self.uki.as_ref()
    }

    /// Returns the kernel component identity for V2 manifests, or `None` for V1.
    #[must_use]
    pub const fn kernel_component(&self) -> Option<&ComponentIdentity> {
        self.kernel_component.as_ref()
    }

    /// Returns the initrd component identity for V2 manifests, or `None` for V1.
    #[must_use]
    pub const fn initrd_component(&self) -> Option<&ComponentIdentity> {
        self.initrd_component.as_ref()
    }

    /// Returns the dm-verity binding for V2 manifests, or `None` for V1.
    #[must_use]
    pub const fn dm_verity(&self) -> Option<&DmVerityBinding> {
        self.dm_verity.as_ref()
    }

    /// Encodes the single canonical JSON representation, ending in a newline.
    ///
    /// # Errors
    ///
    /// Returns an error if this manifest no longer satisfies the schema or
    /// JSON encoding unexpectedly fails.
    pub fn canonical_bytes(&self) -> ImageResult<Vec<u8>> {
        self.validate()?;
        let mut bytes =
            serde_json::to_vec(self).map_err(|error| ImageError::Encoding(error.to_string()))?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    /// Decodes only canonical manifests, rejecting alternate JSON spellings.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed, unsupported, semantically invalid, or
    /// non-canonical manifest bytes.
    pub fn from_canonical_bytes(bytes: &[u8]) -> ImageResult<Self> {
        let manifest: Self = serde_json::from_slice(bytes)
            .map_err(|error| ImageError::Encoding(error.to_string()))?;
        manifest.validate()?;
        if manifest.canonical_bytes()? != bytes {
            return Err(ImageError::NonCanonicalManifest);
        }
        Ok(manifest)
    }

    /// Atomically replaces a manifest file with canonical bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the manifest is invalid or the temporary file,
    /// replacement, or durability sync cannot complete.
    pub fn write_atomic(&self, path: impl AsRef<Path>) -> ImageResult<()> {
        let path = path.as_ref();
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)
            .map_err(|error| ImageError::Io(format!("{}: {error}", parent.display())))?;
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| ImageError::InvalidField {
                field: "manifest output",
                reason: "output must have a UTF-8 file name".to_owned(),
            })?;
        let (temporary, mut file) = create_temporary(parent, file_name)?;
        let result = (|| -> ImageResult<()> {
            file.write_all(&self.canonical_bytes()?)
                .and_then(|()| file.sync_all())
                .map_err(|error| ImageError::Io(format!("{}: {error}", temporary.display())))?;
            fs::rename(&temporary, path)
                .map_err(|error| ImageError::Io(format!("{}: {error}", path.display())))?;
            File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|error| ImageError::Io(format!("{}: {error}", parent.display())))
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    /// Re-hashes every artifact and rejects any content or length mismatch.
    ///
    /// Format 2 requires `uki_path`; omitting it is an error because partial
    /// verification must never be reported as a verified deployment.
    ///
    /// # Errors
    ///
    /// Returns an error if the manifest is invalid, an artifact cannot be
    /// read, or any artifact differs from its recorded binding.
    pub fn verify_artifacts(
        &self,
        paths: &ArtifactPaths,
        uki_path: Option<&Path>,
    ) -> ImageResult<()> {
        self.validate()?;
        let actual = DeploymentArtifacts::from_paths(paths)?;
        if actual.kernel != self.artifacts.kernel {
            return Err(ImageError::ArtifactMismatch("kernel"));
        }
        if actual.initrd != self.artifacts.initrd {
            return Err(ImageError::ArtifactMismatch("initrd"));
        }
        if actual.root_image != self.artifacts.root_image {
            return Err(ImageError::ArtifactMismatch("root image"));
        }
        match (&self.uki, uki_path) {
            (Some(expected), Some(path)) => {
                if UkiBinding::from_path(path)? != *expected {
                    return Err(ImageError::ArtifactMismatch("UKI"));
                }
            }
            (Some(_), None) => {
                return Err(ImageError::InvalidField {
                    field: "uki path",
                    reason: "format 2 verification requires the complete UKI".to_owned(),
                });
            }
            (None, Some(_)) => {
                return Err(ImageError::InvalidField {
                    field: "uki path",
                    reason: "format 1 does not bind a UKI".to_owned(),
                });
            }
            (None, None) => {}
        }
        Ok(())
    }

    /// Returns the physical A/B slot identity.
    #[must_use]
    pub const fn slot(&self) -> DeploymentSlot {
        self.slot
    }

    /// Returns the monotonic slot generation.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns the release's display version; it is not used for ordering.
    #[must_use]
    pub fn system_version(&self) -> &str {
        &self.system_version
    }

    /// Returns all runtime descriptors sorted by major.
    #[must_use]
    pub fn runtimes(&self) -> &[RuntimeDescriptor] {
        &self.runtimes
    }

    /// Returns the bound deployment artifacts.
    #[must_use]
    pub const fn artifacts(&self) -> &DeploymentArtifacts {
        &self.artifacts
    }

    fn validate(&self) -> ImageResult<()> {
        if self.format != MANIFEST_FORMAT_V1 && self.format != MANIFEST_FORMAT_V2 {
            return Err(ImageError::UnsupportedFormat(self.format));
        }
        if self.generation == 0 {
            return Err(ImageError::InvalidField {
                field: "generation",
                reason: "generation must be greater than zero".to_owned(),
            });
        }
        validate_system_version(&self.system_version)?;
        self.artifacts.validate()?;
        if self.runtimes.is_empty() {
            return Err(ImageError::InvalidField {
                field: "runtimes",
                reason: "at least one runtime descriptor is required".to_owned(),
            });
        }
        let mut previous_major = None;
        for runtime in &self.runtimes {
            let major = runtime.validate()?;
            if previous_major.is_some_and(|previous| previous >= major) {
                return Err(ImageError::InvalidField {
                    field: "runtimes",
                    reason: "runtime majors must be sorted and unique".to_owned(),
                });
            }
            previous_major = Some(major);
        }
        if self.format == MANIFEST_FORMAT_V2 {
            let uki_deployment = UkiDeploymentBinding {
                uki: self.uki.clone().ok_or_else(|| ImageError::InvalidField {
                    field: "uki",
                    reason: "format 2 requires a UKI binding".to_owned(),
                })?,
                kernel_component: self.kernel_component.clone().ok_or_else(|| {
                    ImageError::InvalidField {
                        field: "kernel_component",
                        reason: "format 2 requires a kernel component identity".to_owned(),
                    }
                })?,
                initrd_component: self.initrd_component.clone().ok_or_else(|| {
                    ImageError::InvalidField {
                        field: "initrd_component",
                        reason: "format 2 requires an initrd component identity".to_owned(),
                    }
                })?,
                dm_verity: self
                    .dm_verity
                    .clone()
                    .ok_or_else(|| ImageError::InvalidField {
                        field: "dm_verity",
                        reason: "format 2 requires a dm-verity binding".to_owned(),
                    })?,
            };
            uki_deployment.validate()?;
        } else if self.uki.is_some()
            || self.kernel_component.is_some()
            || self.initrd_component.is_some()
            || self.dm_verity.is_some()
        {
            return Err(ImageError::InvalidField {
                field: "format",
                reason: "V1 manifest must not carry UKI or dm-verity fields".to_owned(),
            });
        }
        Ok(())
    }
}

fn create_temporary(parent: &Path, file_name: &str) -> ImageResult<(PathBuf, File)> {
    for attempt in 0_u8..16 {
        let path = parent.join(format!(".{file_name}.{}.{attempt}.tmp", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(ImageError::Io(format!("{}: {error}", path.display())));
            }
        }
    }
    Err(ImageError::Io(
        "no unused manifest temporary path is available".to_owned(),
    ))
}

/// Hashes artifact paths and constructs a complete manifest.
///
/// # Errors
///
/// Returns an error if an artifact cannot be bound or a manifest input fails
/// validation.
pub fn build_manifest(
    slot: DeploymentSlot,
    generation: u64,
    system_version: impl Into<String>,
    paths: &ArtifactPaths,
    runtimes: Vec<RuntimeDescriptor>,
) -> ImageResult<DeploymentManifest> {
    DeploymentManifest::new(
        slot,
        generation,
        system_version,
        DeploymentArtifacts::from_paths(paths)?,
        runtimes,
    )
}

/// Hashes artifact paths plus a UKI and constructs a UKI-aware V2 manifest.
///
/// # Errors
///
/// Returns an error if an artifact or the UKI cannot be bound or a manifest
/// input fails validation.
pub fn build_manifest_v2(
    slot: DeploymentSlot,
    generation: u64,
    system_version: impl Into<String>,
    paths: &ArtifactPaths,
    runtimes: Vec<RuntimeDescriptor>,
    uki_deployment: UkiDeploymentBinding,
) -> ImageResult<DeploymentManifest> {
    DeploymentManifest::new_v2(
        slot,
        generation,
        system_version,
        DeploymentArtifacts::from_paths(paths)?,
        runtimes,
        uki_deployment,
    )
}

fn validate_runtime_name(name: &str) -> ImageResult<u32> {
    let suffix = name
        .strip_prefix("sol-runtime-")
        .ok_or_else(|| ImageError::InvalidField {
            field: "runtime.name",
            reason: "expected sol-runtime-<positive major>".to_owned(),
        })?;
    if suffix.is_empty()
        || suffix.starts_with('0')
        || !suffix.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(ImageError::InvalidField {
            field: "runtime.name",
            reason: "major must be a canonical positive decimal integer".to_owned(),
        });
    }
    suffix
        .parse::<u32>()
        .map_err(|error| ImageError::InvalidField {
            field: "runtime.name",
            reason: error.to_string(),
        })
}

fn runtime_major(name: &str) -> u32 {
    name.strip_prefix("sol-runtime-")
        .and_then(|suffix| suffix.parse().ok())
        .unwrap_or(u32::MAX)
}

fn validate_feature(feature: &str) -> ImageResult<()> {
    let valid = !feature.is_empty()
        && feature.len() <= 128
        && feature.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
        })
        && feature
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && feature
            .bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_alphanumeric());
    if !valid {
        return Err(ImageError::InvalidField {
            field: "runtime.feature",
            reason: "expected 1-128 lowercase ASCII letters, digits, '.', '-' or '_'".to_owned(),
        });
    }
    Ok(())
}

fn validate_system_version(version: &str) -> ImageResult<()> {
    let valid = !version.is_empty()
        && version.len() <= 128
        && version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+' | b'_'));
    if !valid {
        return Err(ImageError::InvalidField {
            field: "system_version",
            reason: "expected 1-128 ASCII release characters".to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn fixture_paths() -> (tempfile::TempDir, ArtifactPaths) {
        let directory = tempdir().expect("fixture directory");
        let paths = ArtifactPaths {
            kernel: directory.path().join("vmlinuz"),
            initrd: directory.path().join("initrd"),
            root_image: directory.path().join("root.img"),
        };
        fs::write(&paths.kernel, b"kernel-v1").expect("kernel fixture");
        fs::write(&paths.initrd, b"initrd-v1").expect("initrd fixture");
        fs::write(&paths.root_image, b"immutable-root-v1").expect("root fixture");
        (directory, paths)
    }

    fn runtime_one(features: &[&str]) -> RuntimeDescriptor {
        RuntimeDescriptor::new(
            "sol-runtime-1",
            12,
            features.iter().map(ToString::to_string).collect(),
        )
        .expect("runtime fixture")
    }

    #[test]
    fn equal_inputs_produce_identical_canonical_bytes() {
        let (_directory, paths) = fixture_paths();
        let first = build_manifest(
            DeploymentSlot::B,
            42,
            "0.2.0-dev",
            &paths,
            vec![
                RuntimeDescriptor::new("sol-runtime-2", 1, vec!["shell.menu-v1".to_owned()])
                    .expect("runtime two"),
                runtime_one(&["documents.v2", "accessibility.tree-v1"]),
            ],
        )
        .expect("first manifest");
        let second = build_manifest(
            DeploymentSlot::B,
            42,
            "0.2.0-dev",
            &paths,
            vec![
                runtime_one(&["accessibility.tree-v1", "documents.v2"]),
                RuntimeDescriptor::new("sol-runtime-2", 1, vec!["shell.menu-v1".to_owned()])
                    .expect("runtime two"),
            ],
        )
        .expect("second manifest");

        assert_eq!(first.canonical_bytes(), second.canonical_bytes());
        assert_eq!(first.runtimes()[0].name(), "sol-runtime-1");
        assert_eq!(first.runtimes()[0].features()[0], "accessibility.tree-v1");
    }

    #[test]
    fn canonical_manifest_round_trips_exactly() {
        let (_directory, paths) = fixture_paths();
        let manifest = build_manifest(
            DeploymentSlot::A,
            7,
            "2026.08.23",
            &paths,
            vec![runtime_one(&["documents.v2"])],
        )
        .expect("manifest");
        let bytes = manifest.canonical_bytes().expect("canonical bytes");

        assert_eq!(
            DeploymentManifest::from_canonical_bytes(&bytes).expect("decode"),
            manifest
        );

        let pretty = serde_json::to_string_pretty(&manifest).expect("pretty JSON");
        assert_eq!(
            DeploymentManifest::from_canonical_bytes(pretty.as_bytes()),
            Err(ImageError::NonCanonicalManifest)
        );
    }

    #[test]
    fn every_bound_artifact_is_verified() {
        let (_directory, paths) = fixture_paths();
        let manifest = build_manifest(
            DeploymentSlot::A,
            1,
            "1.0.0",
            &paths,
            vec![runtime_one(&[])],
        )
        .expect("manifest");
        manifest
            .verify_artifacts(&paths, None)
            .expect("original artifacts");

        for (path, expected) in [
            (&paths.kernel, "kernel"),
            (&paths.initrd, "initrd"),
            (&paths.root_image, "root image"),
        ] {
            let original = fs::read(path).expect("fixture contents");
            fs::write(path, b"tampered").expect("tamper fixture");
            assert_eq!(
                manifest.verify_artifacts(&paths, None),
                Err(ImageError::ArtifactMismatch(expected))
            );
            fs::write(path, original).expect("restore fixture");
        }
    }

    #[test]
    fn slot_generation_and_runtime_contract_change_identity() {
        let (_directory, paths) = fixture_paths();
        let artifacts = DeploymentArtifacts::from_paths(&paths).expect("artifacts");
        let build = |slot, generation, revision| {
            DeploymentManifest::new(
                slot,
                generation,
                "1.0.0",
                artifacts.clone(),
                vec![RuntimeDescriptor::new("sol-runtime-1", revision, vec![]).expect("runtime")],
            )
            .expect("manifest")
            .canonical_bytes()
            .expect("bytes")
        };

        assert_ne!(
            build(DeploymentSlot::A, 1, 1),
            build(DeploymentSlot::B, 1, 1)
        );
        assert_ne!(
            build(DeploymentSlot::A, 1, 1),
            build(DeploymentSlot::A, 2, 1)
        );
        assert_ne!(
            build(DeploymentSlot::A, 1, 1),
            build(DeploymentSlot::A, 1, 2)
        );
    }

    #[test]
    fn invalid_or_ambiguous_identity_is_rejected() {
        let (_directory, paths) = fixture_paths();
        let artifacts = DeploymentArtifacts::from_paths(&paths).expect("artifacts");
        assert!(RuntimeDescriptor::new("runtime-1", 1, vec![]).is_err());
        assert!(RuntimeDescriptor::new("sol-runtime-01", 1, vec![]).is_err());
        assert!(RuntimeDescriptor::new("sol-runtime-1", 0, vec![]).is_err());
        assert!(
            RuntimeDescriptor::new("sol-runtime-1", 1, vec!["Bad Feature".to_owned()]).is_err()
        );
        assert!(
            DeploymentManifest::new(
                DeploymentSlot::A,
                0,
                "1.0.0",
                artifacts.clone(),
                vec![runtime_one(&[])]
            )
            .is_err()
        );
        assert!(
            DeploymentManifest::new(
                DeploymentSlot::A,
                1,
                "1.0.0",
                artifacts,
                vec![runtime_one(&[]), runtime_one(&["documents.v2"])]
            )
            .is_err()
        );
    }

    #[test]
    fn manifest_write_is_canonical_and_atomic() {
        let (directory, paths) = fixture_paths();
        let manifest = build_manifest(
            DeploymentSlot::B,
            9,
            "1.2.3",
            &paths,
            vec![runtime_one(&[])],
        )
        .expect("manifest");
        let output = directory.path().join("deployments/B/manifest.json");

        manifest.write_atomic(&output).expect("write manifest");
        let bytes = fs::read(&output).expect("read manifest");
        assert_eq!(bytes, manifest.canonical_bytes().expect("canonical bytes"));
        assert_eq!(
            DeploymentManifest::from_canonical_bytes(&bytes).expect("decode"),
            manifest
        );
    }

    fn v2_fixture_paths() -> (tempfile::TempDir, ArtifactPaths) {
        let directory = tempdir().expect("fixture directory");
        let paths = ArtifactPaths {
            kernel: directory.path().join("vmlinuz"),
            initrd: directory.path().join("initrd"),
            root_image: directory.path().join("root.img"),
        };
        fs::write(&paths.kernel, b"kernel-v2").expect("kernel fixture");
        fs::write(&paths.initrd, b"initrd-v2").expect("initrd fixture");
        fs::write(&paths.root_image, b"immutable-root-v2").expect("root fixture");
        fs::write(directory.path().join("uki.efi"), b"uki-contents-v2").expect("uki fixture");
        (directory, paths)
    }

    fn v2_binding(directory: &Path) -> UkiDeploymentBinding {
        UkiDeploymentBinding::from_path(
            directory.join("uki.efi"),
            ComponentIdentity::new("kernel-x86_64-6.9", "slot-b-gen-42-kernel-abc123")
                .expect("kernel component"),
            ComponentIdentity::new("initrd-base-2026", "slot-b-gen-42-initrd-def456")
                .expect("initrd component"),
            DmVerityBinding::new("a".repeat(64), "slot-b-root-abc123").expect("dm-verity binding"),
        )
        .expect("UKI deployment binding")
    }

    #[test]
    fn v2_manifest_round_trips_exactly() {
        let (directory, paths) = v2_fixture_paths();
        let uki_path = directory.path().join("uki.efi");

        let manifest = build_manifest_v2(
            DeploymentSlot::B,
            42,
            "0.3.0-dev",
            &paths,
            vec![runtime_one(&["documents.v2"])],
            v2_binding(directory.path()),
        )
        .expect("V2 manifest");
        let bytes = manifest.canonical_bytes().expect("canonical bytes");

        assert_eq!(manifest.format_version(), MANIFEST_FORMAT_V2);
        assert_eq!(manifest.manifest_format(), Some(ManifestFormat::V2));
        assert!(manifest.uki().is_some());
        assert!(manifest.kernel_component().is_some());
        assert!(manifest.initrd_component().is_some());
        assert!(manifest.dm_verity().is_some());

        let decoded = DeploymentManifest::from_canonical_bytes(&bytes).expect("decode V2 manifest");
        assert_eq!(decoded, manifest);
        decoded
            .verify_artifacts(&paths, Some(&uki_path))
            .expect("complete V2 artifacts");
    }

    #[test]
    fn v1_and_v2_produce_distinct_bytes() {
        let (directory, paths) = v2_fixture_paths();

        let v1 = build_manifest(
            DeploymentSlot::B,
            42,
            "0.3.0-dev",
            &paths,
            vec![runtime_one(&["documents.v2"])],
        )
        .expect("V1 manifest");
        let v2 = build_manifest_v2(
            DeploymentSlot::B,
            42,
            "0.3.0-dev",
            &paths,
            vec![runtime_one(&["documents.v2"])],
            v2_binding(directory.path()),
        )
        .expect("V2 manifest");

        assert_ne!(
            v1.canonical_bytes().expect("V1 bytes"),
            v2.canonical_bytes().expect("V2 bytes")
        );
    }

    #[test]
    fn v2_verification_requires_and_checks_the_complete_uki() {
        let (directory, paths) = v2_fixture_paths();
        let uki_path = directory.path().join("uki.efi");

        let manifest = build_manifest_v2(
            DeploymentSlot::B,
            42,
            "0.3.0-dev",
            &paths,
            vec![runtime_one(&["documents.v2"])],
            v2_binding(directory.path()),
        )
        .expect("V2 manifest");

        assert!(matches!(
            manifest.verify_artifacts(&paths, None),
            Err(ImageError::InvalidField {
                field: "uki path",
                ..
            })
        ));
        fs::write(&uki_path, b"tampered-uki").expect("tamper UKI");
        assert_eq!(
            manifest.verify_artifacts(&paths, Some(&uki_path)),
            Err(ImageError::ArtifactMismatch("UKI"))
        );
    }

    #[test]
    fn v2_decode_rejects_missing_or_invalid_fields() {
        let (directory, paths) = v2_fixture_paths();
        let manifest = build_manifest_v2(
            DeploymentSlot::B,
            42,
            "0.3.0-dev",
            &paths,
            vec![runtime_one(&[])],
            v2_binding(directory.path()),
        )
        .expect("V2 manifest");
        let mut value = serde_json::to_value(&manifest).expect("manifest JSON");
        value
            .as_object_mut()
            .expect("manifest object")
            .remove("dm_verity");
        let mut missing = serde_json::to_vec(&value).expect("missing-field JSON");
        missing.push(b'\n');
        assert!(matches!(
            DeploymentManifest::from_canonical_bytes(&missing),
            Err(ImageError::InvalidField {
                field: "dm_verity",
                ..
            })
        ));

        let object = value.as_object_mut().expect("manifest object");
        object.insert(
            "dm_verity".to_owned(),
            serde_json::json!({
                "root_hash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "slot_root_identity": "slot-b-root-abc123"
            }),
        );
        object
            .get_mut("kernel_component")
            .and_then(serde_json::Value::as_object_mut)
            .expect("kernel component")
            .insert("name".to_owned(), serde_json::json!("Bad Name"));
        let mut invalid = serde_json::to_vec(&value).expect("invalid-field JSON");
        invalid.push(b'\n');
        assert!(matches!(
            DeploymentManifest::from_canonical_bytes(&invalid),
            Err(ImageError::InvalidField {
                field: "component.name",
                ..
            })
        ));
    }

    #[test]
    fn component_identity_validates() {
        assert!(ComponentIdentity::new("", "slot-b-kernel").is_err());
        assert!(ComponentIdentity::new("ok", "").is_err());
        assert!(ComponentIdentity::new("Bad Name", "slot-b-kernel").is_err());
        let good =
            ComponentIdentity::new("kernel-x86_64", "slot-b-kernel-abc").expect("valid component");
        assert_eq!(good.name(), "kernel-x86_64");
        assert_eq!(good.slot_identity(), "slot-b-kernel-abc");
    }

    #[test]
    fn dm_verity_binding_validates() {
        assert!(DmVerityBinding::new("", "slot-b-root").is_err());
        assert!(DmVerityBinding::new("g".repeat(64), "").is_err());
        assert!(DmVerityBinding::new("ZZ".repeat(32), "slot-b-root").is_err());
        let good =
            DmVerityBinding::new("a".repeat(64), "slot-b-root-abc123").expect("valid dm-verity");
        assert_eq!(good.root_hash(), "a".repeat(64).as_str());
        assert_eq!(good.slot_root_identity(), "slot-b-root-abc123");
    }
}
