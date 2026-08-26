//! Signing, verification, publisher lineage, and update identity for SOL `.app` bundles.
//!
//! The implementation follows ADR-0029: every regular bundle file is bound by a
//! canonical JSON content manifest, release signatures use the protobuf v2
//! block, and publisher rotation uses independently verified lineage protobufs.

mod error;
mod key;
mod lineage;
mod manifest;
pub mod proto;
mod revocation;

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub use error::{BundleError, Result};
pub use key::{PrivateKey, fingerprint};
pub use lineage::{
    MAX_LINEAGE_LENGTH, MAX_LINEAGE_VERIFY_TIME, VerifiedLineage, initial_lineage, lineage_extends,
    rotate_lineage, verify_lineage,
};
pub use manifest::{AppIdentity, ContentManifest, read_app_identity};
use prost::Message;
pub use proto::{PublisherLineage, RotationMetadata, SignatureAlgorithm};
pub use revocation::{CacheState, RevocationCache, RevocationEntry};
use sha2::{Digest as _, Sha256};

use crate::proto::{SignedData, Signer, SolSignatureV2};

const SIGNATURE_DIRECTORY: &str = ".signatures";
const CONTENT_MANIFEST_FILE: &str = "manifest.json";
const V2_SIGNATURE_FILE: &str = "v2.sig";
const LINEAGES_DIRECTORY: &str = "lineages";

/// A release signer whose signature and complete lineage have been verified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedSigner {
    /// Algorithm-specific current public key.
    pub public_key: Vec<u8>,
    /// Verified root-to-current publisher history.
    pub lineage: VerifiedLineage,
    /// Signature creation time as Unix epoch seconds UTC.
    pub signed_at: i64,
    /// Release signature algorithm.
    pub algorithm: SignatureAlgorithm,
}

/// Security identity returned only after complete all-or-nothing verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedIdentity {
    /// Durable application ID.
    pub app_id: String,
    /// Human-readable release version.
    pub version: String,
    /// Monotonic anti-replay version.
    pub version_code: u64,
    /// Primary signer publisher lineage.
    pub publisher_lineage: VerifiedLineage,
    /// SHA-256 binding of canonical manifest, signature block, and lineages.
    pub bundle_hash: String,
    /// Primary signature time.
    pub signed_at: i64,
    /// Every verified signer; none may fail.
    pub all_signers: Vec<VerifiedSigner>,
    /// Minimum supported SOL release.
    pub min_sol_version: u32,
}

/// Permission-grant continuity result for an app update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrantInheritance {
    /// Same App ID and an exact extension of the primary publisher lineage.
    SameLineage {
        /// Historical publisher root fingerprint.
        old_root: String,
        /// Candidate current publisher key fingerprint.
        new_current: String,
    },
    /// App ID or publisher history changed.
    Discontinuous,
}

/// Generates and writes a publisher key.
///
/// # Errors
///
/// Returns an error on key generation, encoding, or output failure.
pub fn generate_key(path: &Path, algorithm: SignatureAlgorithm) -> Result<String> {
    let key = PrivateKey::generate(algorithm)?;
    key.write(path)?;
    Ok(fingerprint(&key.public_key()?))
}

/// Signs a bundle from scratch, replacing any prior signature block.
///
/// # Errors
///
/// Returns an error for invalid content, keys, lineage, timestamp, or filesystem
/// operations, or if post-write verification fails.
pub fn sign_bundle(
    bundle: &Path,
    key: &PrivateKey,
    lineage: Option<&PublisherLineage>,
    min_sol_version: u32,
    timestamp: i64,
) -> Result<VerifiedIdentity> {
    validate_timestamp(timestamp)?;
    let manifest = ContentManifest::build(bundle)?;
    let manifest_bytes = manifest.canonical_bytes()?;
    let public_key = key.public_key()?;
    let signer_lineage = lineage
        .cloned()
        .unwrap_or_else(|| initial_lineage(public_key.clone()));
    let verified_lineage = verify_lineage(&signer_lineage)?;
    if verified_lineage.current_key != public_key {
        return Err(BundleError::SignerLineageMismatch);
    }
    let signer = create_signer(&manifest, &manifest_bytes, key, timestamp)?;
    let block = SolSignatureV2 {
        signers: vec![signer],
        min_sol_version,
    };
    let mut lineages = BTreeMap::new();
    if lineage.is_some() {
        lineages.insert(0, signer_lineage);
    }
    replace_signature_block(bundle, &manifest_bytes, &block.encode_to_vec(), &lineages)?;
    verify_app_bundle(bundle, None)
}

/// Adds a fully independent signer and corresponding lineage to a signed bundle.
///
/// # Errors
///
/// Returns an error unless the existing bundle verifies completely and the new
/// signer and lineage are valid.
pub fn add_signer(
    bundle: &Path,
    key: &PrivateKey,
    lineage: Option<&PublisherLineage>,
    timestamp: i64,
) -> Result<VerifiedIdentity> {
    validate_timestamp(timestamp)?;
    let _ = verify_app_bundle(bundle, None)?;
    let signature_dir = bundle.join(SIGNATURE_DIRECTORY);
    let manifest_bytes = read_regular_file(&signature_dir.join(CONTENT_MANIFEST_FILE))?;
    let manifest = ContentManifest::from_canonical_bytes(&manifest_bytes)?;
    let block_bytes = read_regular_file(&signature_dir.join(V2_SIGNATURE_FILE))?;
    let mut block = decode_canonical::<SolSignatureV2>(&block_bytes, "v2.sig")?;
    let new_public = key.public_key()?;
    let new_lineage = lineage
        .cloned()
        .unwrap_or_else(|| initial_lineage(new_public.clone()));
    if verify_lineage(&new_lineage)?.current_key != new_public {
        return Err(BundleError::SignerLineageMismatch);
    }
    block
        .signers
        .push(create_signer(&manifest, &manifest_bytes, key, timestamp)?);

    let mut lineages = BTreeMap::new();
    for index in 0..block.signers.len() - 1 {
        let path = signature_dir
            .join(LINEAGES_DIRECTORY)
            .join(format!("{index}.bin"));
        let existing = if path.exists() {
            read_lineage(&path)?
        } else {
            initial_lineage(block.signers[index].public_key.clone())
        };
        lineages.insert(index, existing);
    }
    lineages.insert(block.signers.len() - 1, new_lineage);
    replace_signature_block(bundle, &manifest_bytes, &block.encode_to_vec(), &lineages)?;
    verify_app_bundle(bundle, None)
}

/// Verifies bundle content, every signer, every lineage, and optional revocation state.
///
/// # Errors
///
/// Returns an error for any malformed input, content mismatch, invalid signer or
/// lineage, unsafe layout, or effective key revocation.
pub fn verify_app_bundle(
    bundle: &Path,
    revocations: Option<&RevocationCache>,
) -> Result<VerifiedIdentity> {
    let signature_dir = bundle.join(SIGNATURE_DIRECTORY);
    validate_signature_directory(&signature_dir)?;
    let manifest_bytes = read_regular_file(&signature_dir.join(CONTENT_MANIFEST_FILE))?;
    let manifest = ContentManifest::from_canonical_bytes(&manifest_bytes)?;
    manifest.verify_content(bundle)?;
    let signature_bytes = read_regular_file(&signature_dir.join(V2_SIGNATURE_FILE))?;
    let block = decode_canonical::<SolSignatureV2>(&signature_bytes, "v2.sig")?;
    if block.signers.is_empty() {
        return Err(BundleError::NoSigners);
    }
    validate_lineage_files(&signature_dir, block.signers.len())?;

    let manifest_digest = Sha256::digest(&manifest_bytes).to_vec();
    let content_digest = manifest.content_digest()?;
    let mut verified_signers = Vec::with_capacity(block.signers.len());
    let mut lineage_bytes = Vec::new();
    for (index, signer) in block.signers.iter().enumerate() {
        let verified = verify_release_signer(
            signer,
            index,
            &manifest,
            &manifest_digest,
            &content_digest,
            &signature_dir,
            block.signers.len(),
        )?;
        if let Some(cache) = revocations {
            cache.check_signer(&verified)?;
        }
        let lineage_path = signature_dir
            .join(LINEAGES_DIRECTORY)
            .join(format!("{index}.bin"));
        if lineage_path.exists() {
            lineage_bytes.push((index, read_regular_file(&lineage_path)?));
        }
        verified_signers.push(verified);
    }
    let primary = verified_signers.first().ok_or(BundleError::NoSigners)?;
    let bundle_hash = compute_bundle_hash(&manifest_bytes, &signature_bytes, &lineage_bytes);
    Ok(VerifiedIdentity {
        app_id: manifest.app_id,
        version: manifest.version,
        version_code: manifest.version_code,
        publisher_lineage: primary.lineage.clone(),
        bundle_hash,
        signed_at: primary.signed_at,
        all_signers: verified_signers,
        min_sol_version: block.min_sol_version,
    })
}

/// Checks publisher continuity without applying the anti-replay update policy.
#[must_use]
pub fn check_grant_inheritance(old: &VerifiedIdentity, new: &VerifiedIdentity) -> GrantInheritance {
    if old.app_id != new.app_id || !lineage_extends(&new.publisher_lineage, &old.publisher_lineage)
    {
        return GrantInheritance::Discontinuous;
    }
    GrantInheritance::SameLineage {
        old_root: fingerprint(&old.publisher_lineage.root_key),
        new_current: fingerprint(&new.publisher_lineage.current_key),
    }
}

/// Enforces a strictly increasing `version_code` and returns grant continuity.
///
/// # Errors
///
/// Returns [`BundleError::DowngradeAttempt`] unless the candidate version code
/// is strictly greater than the installed version code.
pub fn check_update(
    installed: &VerifiedIdentity,
    candidate: &VerifiedIdentity,
) -> Result<GrantInheritance> {
    if candidate.version_code <= installed.version_code {
        return Err(BundleError::DowngradeAttempt {
            installed: installed.version_code,
            candidate: candidate.version_code,
        });
    }
    Ok(check_grant_inheritance(installed, candidate))
}

/// Reads a canonical lineage protobuf.
///
/// # Errors
///
/// Returns an error for file I/O, malformed protobuf, or non-canonical encoding.
pub fn read_lineage(path: &Path) -> Result<PublisherLineage> {
    let bytes = read_regular_file(path)?;
    decode_canonical(&bytes, "PublisherLineage")
}

/// Writes a canonical lineage protobuf atomically.
///
/// # Errors
///
/// Returns an error if lineage verification or atomic output fails.
pub fn write_lineage(path: &Path, lineage: &PublisherLineage) -> Result<()> {
    verify_lineage(lineage)?;
    write_atomic_file(path, &lineage.encode_to_vec())
}

/// Current Unix timestamp in UTC seconds.
///
/// # Errors
///
/// Returns an error if the system clock precedes the Unix epoch or exceeds `i64`.
pub fn unix_timestamp_now() -> Result<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| BundleError::Encoding {
            kind: "system clock",
            message: error.to_string(),
        })?;
    i64::try_from(duration.as_secs()).map_err(|_| BundleError::Encoding {
        kind: "system clock",
        message: "timestamp exceeds i64".to_owned(),
    })
}

fn create_signer(
    manifest: &ContentManifest,
    manifest_bytes: &[u8],
    key: &PrivateKey,
    timestamp: i64,
) -> Result<Signer> {
    let signed_data = SignedData {
        app_id: manifest.app_id.clone(),
        version: manifest.version.clone(),
        version_code: manifest.version_code,
        manifest_digest: Sha256::digest(manifest_bytes).to_vec(),
        content_digest: manifest.content_digest()?,
        timestamp,
        additional_digests: Vec::new(),
    }
    .encode_to_vec();
    Ok(Signer {
        signed_data: signed_data.clone(),
        signatures: vec![key.sign(&signed_data)],
        public_key: key.public_key()?,
        certificate: None,
    })
}

fn verify_release_signer(
    signer: &Signer,
    index: usize,
    manifest: &ContentManifest,
    manifest_digest: &[u8],
    content_digest: &[u8],
    signature_dir: &Path,
    signer_count: usize,
) -> Result<VerifiedSigner> {
    if signer.signatures.is_empty() {
        return Err(BundleError::NoSignatures(index));
    }
    let release_data = decode_canonical::<SignedData>(&signer.signed_data, "SignedData")?;
    validate_timestamp(release_data.timestamp)?;
    if release_data.app_id != manifest.app_id {
        return Err(BundleError::SignedDataMismatch("app_id"));
    }
    if release_data.version != manifest.version {
        return Err(BundleError::SignedDataMismatch("version"));
    }
    if release_data.version_code != manifest.version_code {
        return Err(BundleError::SignedDataMismatch("version_code"));
    }
    if release_data.manifest_digest != manifest_digest {
        return Err(BundleError::SignedDataMismatch("manifest_digest"));
    }
    if release_data.content_digest != content_digest {
        return Err(BundleError::SignedDataMismatch("content_digest"));
    }
    let first_algorithm = signer.signatures[0].algorithm;
    for signature in &signer.signatures {
        if signature.algorithm != first_algorithm {
            return Err(BundleError::InvalidSignature {
                signer: index,
                message: "one public key cannot declare multiple algorithms".to_owned(),
            });
        }
        key::verify(&signer.public_key, signature, &signer.signed_data).map_err(|error| {
            BundleError::InvalidSignature {
                signer: index,
                message: error.to_string(),
            }
        })?;
    }
    let algorithm = SignatureAlgorithm::try_from(first_algorithm)
        .map_err(|_| BundleError::UnsupportedAlgorithm(first_algorithm))?;
    let lineage_path = signature_dir
        .join(LINEAGES_DIRECTORY)
        .join(format!("{index}.bin"));
    let lineage = if lineage_path.exists() {
        read_lineage(&lineage_path)?
    } else if signer_count == 1 {
        initial_lineage(signer.public_key.clone())
    } else {
        return Err(BundleError::InvalidLineage(format!(
            "multi-signer bundle is missing lineages/{index}.bin"
        )));
    };
    let verified_lineage = verify_lineage(&lineage)?;
    if verified_lineage.current_key != signer.public_key {
        return Err(BundleError::SignerLineageMismatch);
    }
    Ok(VerifiedSigner {
        public_key: signer.public_key.clone(),
        lineage: verified_lineage,
        signed_at: release_data.timestamp,
        algorithm,
    })
}

fn validate_timestamp(timestamp: i64) -> Result<()> {
    if timestamp < 0 {
        return Err(BundleError::Encoding {
            kind: "timestamp",
            message: "must be Unix epoch UTC seconds greater than or equal to zero".to_owned(),
        });
    }
    Ok(())
}

fn decode_canonical<M>(bytes: &[u8], kind: &'static str) -> Result<M>
where
    M: Message + Default,
{
    let message = M::decode(bytes).map_err(|error| BundleError::encoding(kind, error))?;
    if message.encode_to_vec() != bytes {
        return Err(BundleError::Encoding {
            kind,
            message: "non-canonical protobuf encoding or unknown fields".to_owned(),
        });
    }
    Ok(message)
}

fn validate_signature_directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(|error| BundleError::io(path, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(BundleError::InvalidLayout(format!(
            "{} must be a real directory",
            path.display()
        )));
    }
    Ok(())
}

fn validate_lineage_files(signature_dir: &Path, signer_count: usize) -> Result<()> {
    let lineages = signature_dir.join(LINEAGES_DIRECTORY);
    if !lineages.exists() {
        if signer_count > 1 {
            return Err(BundleError::InvalidLineage(
                "multi-signer bundle requires a lineages directory".to_owned(),
            ));
        }
        return Ok(());
    }
    let metadata =
        fs::symlink_metadata(&lineages).map_err(|error| BundleError::io(&lineages, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(BundleError::InvalidLayout(
            "lineages must be a real directory".to_owned(),
        ));
    }
    for entry in fs::read_dir(&lineages).map_err(|error| BundleError::io(&lineages, error))? {
        let entry = entry.map_err(|error| BundleError::io(&lineages, error))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let expected = name
            .strip_suffix(".bin")
            .and_then(|value| value.parse::<usize>().ok())
            .is_some_and(|index| index < signer_count);
        if !expected {
            return Err(BundleError::InvalidLayout(format!(
                "unexpected lineage entry {name}"
            )));
        }
        let metadata = entry
            .file_type()
            .map_err(|error| BundleError::io(entry.path(), error))?;
        if !metadata.is_file() {
            return Err(BundleError::InvalidLayout(format!(
                "lineage entry {name} must be a regular file"
            )));
        }
    }
    Ok(())
}

fn replace_signature_block(
    bundle: &Path,
    manifest: &[u8],
    v2: &[u8],
    lineages: &BTreeMap<usize, PublisherLineage>,
) -> Result<()> {
    let target = bundle.join(SIGNATURE_DIRECTORY);
    let staging = bundle.join(format!(".signatures.tmp-{}", std::process::id()));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|error| BundleError::io(&staging, error))?;
    }
    fs::create_dir(&staging).map_err(|error| BundleError::io(&staging, error))?;
    write_atomic_file(&staging.join(CONTENT_MANIFEST_FILE), manifest)?;
    write_atomic_file(&staging.join(V2_SIGNATURE_FILE), v2)?;
    if !lineages.is_empty() {
        let directory = staging.join(LINEAGES_DIRECTORY);
        fs::create_dir(&directory).map_err(|error| BundleError::io(&directory, error))?;
        for (index, lineage) in lineages {
            verify_lineage(lineage)?;
            write_atomic_file(
                &directory.join(format!("{index}.bin")),
                &lineage.encode_to_vec(),
            )?;
        }
    }

    let backup = bundle.join(format!(".signatures.backup-{}", std::process::id()));
    if target.exists() {
        fs::rename(&target, &backup).map_err(|error| BundleError::io(&target, error))?;
    }
    if let Err(error) = fs::rename(&staging, &target) {
        if backup.exists() {
            let _ = fs::rename(&backup, &target);
        }
        return Err(BundleError::io(&staging, error));
    }
    if backup.exists() {
        fs::remove_dir_all(&backup).map_err(|error| BundleError::io(&backup, error))?;
    }
    Ok(())
}

fn compute_bundle_hash(manifest: &[u8], v2: &[u8], lineages: &[(usize, Vec<u8>)]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"SOL-SIGNED-BUNDLE-V2\0");
    update_length_delimited(&mut hasher, manifest);
    update_length_delimited(&mut hasher, v2);
    for (index, bytes) in lineages {
        hasher.update(u64::try_from(*index).unwrap_or(u64::MAX).to_be_bytes());
        update_length_delimited(&mut hasher, bytes);
    }
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

fn update_length_delimited(hasher: &mut Sha256, value: &[u8]) {
    hasher.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    hasher.update(value);
}

fn read_regular_file(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).map_err(|error| BundleError::io(path, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(BundleError::InvalidLayout(format!(
            "{} must be a regular file",
            path.display()
        )));
    }
    fs::read(path).map_err(|error| BundleError::io(path, error))
}

fn write_atomic_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| BundleError::InvalidLayout(format!("{} has no parent", path.display())))?;
    fs::create_dir_all(parent).map_err(|error| BundleError::io(parent, error))?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| BundleError::InvalidLayout("output filename must be UTF-8".to_owned()))?;
    let temporary = parent.join(format!(".{file_name}.tmp-{}", std::process::id()));
    let mut file = File::create(&temporary).map_err(|error| BundleError::io(&temporary, error))?;
    file.write_all(bytes)
        .map_err(|error| BundleError::io(&temporary, error))?;
    file.sync_all()
        .map_err(|error| BundleError::io(&temporary, error))?;
    fs::rename(&temporary, path).map_err(|error| BundleError::io(path, error))
}

pub(crate) fn write_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| BundleError::io(parent, error))?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| BundleError::io(path, error))?;
    file.write_all(bytes)
        .map_err(|error| BundleError::io(path, error))?;
    file.sync_all()
        .map_err(|error| BundleError::io(path, error))
}
