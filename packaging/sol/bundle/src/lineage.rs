use std::collections::HashSet;
use std::time::{Duration, Instant};

use prost::Message as _;

use crate::error::{BundleError, Result};
use crate::key::{PrivateKey, fingerprint, verify};
use crate::proto::{
    PublisherLineage, RotationMetadata, SignatureAlgorithm, SignedSignerConfig, SignerConfig,
};

/// Maximum accepted number of keys in one lineage.
pub const MAX_LINEAGE_LENGTH: usize = 100;
/// Verification time budget checked at every lineage node.
pub const MAX_LINEAGE_VERIFY_TIME: Duration = Duration::from_millis(100);

/// A verified root-to-current publisher key chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedLineage {
    /// First publisher public key.
    pub root_key: Vec<u8>,
    /// Key authorized to sign the current release.
    pub current_key: Vec<u8>,
    /// Complete ordered public-key chain.
    pub chain: Vec<Vec<u8>>,
    /// Signature-covered metadata for every transition.
    pub rotations: Vec<RotationMetadata>,
}

/// Verifies structure, uniqueness, adjacency, declared algorithms, and signatures.
///
/// # Errors
///
/// Returns an error for malformed protobuf state, invalid transitions,
/// duplicate keys, excessive length or time, or failed signatures.
pub fn verify_lineage(lineage: &PublisherLineage) -> Result<VerifiedLineage> {
    if lineage.version != 1 {
        return Err(BundleError::InvalidLineage(format!(
            "unsupported format version {}",
            lineage.version
        )));
    }
    if lineage.signers.is_empty() {
        return Err(BundleError::InvalidLineage(
            "lineage must contain at least one key".to_owned(),
        ));
    }
    if lineage.signers.len() > MAX_LINEAGE_LENGTH {
        return Err(BundleError::InvalidLineage(format!(
            "chain contains {} keys; maximum is {MAX_LINEAGE_LENGTH}",
            lineage.signers.len()
        )));
    }

    let started = Instant::now();
    let mut seen = HashSet::new();
    let mut chain = Vec::with_capacity(lineage.signers.len());
    let mut rotations = Vec::with_capacity(lineage.signers.len().saturating_sub(1));
    for (index, signer) in lineage.signers.iter().enumerate() {
        if started.elapsed() > MAX_LINEAGE_VERIFY_TIME {
            return Err(BundleError::InvalidLineage(
                "verification exceeded 100 ms".to_owned(),
            ));
        }
        if signer.certificate.is_empty() {
            return Err(BundleError::InvalidLineage(format!(
                "node {index} has an empty public key"
            )));
        }
        let key_fingerprint = fingerprint(&signer.certificate);
        if !seen.insert(key_fingerprint) {
            return Err(BundleError::InvalidLineage(format!(
                "duplicate/circular key at node {index}"
            )));
        }

        let is_last = index + 1 == lineage.signers.len();
        if is_last {
            if signer.signed_data.is_some() || !signer.signatures.is_empty() {
                return Err(BundleError::InvalidLineage(
                    "current (last) node must not contain signed_data or signatures".to_owned(),
                ));
            }
        } else {
            let bytes = signer.signed_data.as_deref().ok_or_else(|| {
                BundleError::InvalidLineage(format!("node {index} is missing signed_data"))
            })?;
            let signed = SignedSignerConfig::decode(bytes)
                .map_err(|error| BundleError::encoding("SignedSignerConfig", error))?;
            if signed.next_signer_certificate != lineage.signers[index + 1].certificate {
                return Err(BundleError::InvalidLineage(format!(
                    "node {index} does not name the adjacent next key"
                )));
            }
            if seen.contains(&fingerprint(&signed.next_signer_certificate)) {
                return Err(BundleError::InvalidLineage(format!(
                    "node {index} contains a circular forward reference"
                )));
            }
            let algorithm = SignatureAlgorithm::try_from(signed.algorithm)
                .map_err(|_| BundleError::UnsupportedAlgorithm(signed.algorithm))?;
            if signer.signatures.is_empty() {
                return Err(BundleError::InvalidLineage(format!(
                    "node {index} contains no rotation signature"
                )));
            }
            for signature in &signer.signatures {
                if signature.algorithm != algorithm as i32 {
                    return Err(BundleError::InvalidLineage(format!(
                        "node {index} signature algorithm disagrees with signed_data"
                    )));
                }
                verify(&signer.certificate, signature, bytes).map_err(|error| {
                    BundleError::InvalidLineage(format!(
                        "node {index} rotation signature failed: {error}"
                    ))
                })?;
            }
            rotations.push(signed.metadata.unwrap_or(RotationMetadata {
                reason: String::new(),
                timestamp: 0,
                description: String::new(),
            }));
        }
        chain.push(signer.certificate.clone());
    }
    Ok(VerifiedLineage {
        root_key: chain[0].clone(),
        current_key: chain[chain.len() - 1].clone(),
        chain,
        rotations,
    })
}

/// Creates an implicit initial lineage for an unrotated signer.
#[must_use]
pub fn initial_lineage(public_key: Vec<u8>) -> PublisherLineage {
    PublisherLineage {
        signers: vec![SignerConfig {
            certificate: public_key,
            signed_data: None,
            signatures: Vec::new(),
        }],
        version: 1,
    }
}

/// Extends a lineage by signing the new key with the current private key.
///
/// # Errors
///
/// Returns an error when the existing lineage is invalid, the old key is not
/// current, the new key is duplicated, or key encoding/signing fails.
pub fn rotate_lineage(
    existing: Option<PublisherLineage>,
    old_key: &PrivateKey,
    new_key: &PrivateKey,
    metadata: RotationMetadata,
) -> Result<PublisherLineage> {
    let old_public = old_key.public_key()?;
    let new_public = new_key.public_key()?;
    if old_public == new_public {
        return Err(BundleError::InvalidLineage(
            "new key must differ from the current key".to_owned(),
        ));
    }
    let mut lineage = existing.unwrap_or_else(|| initial_lineage(old_public.clone()));
    let verified = verify_lineage(&lineage)?;
    if verified.current_key != old_public {
        return Err(BundleError::InvalidLineage(
            "--old-key is not the current key of the input lineage".to_owned(),
        ));
    }
    if verified.chain.iter().any(|key| key == &new_public) {
        return Err(BundleError::InvalidLineage(
            "new key already occurs in the lineage".to_owned(),
        ));
    }
    if lineage.signers.len() >= MAX_LINEAGE_LENGTH {
        return Err(BundleError::InvalidLineage(format!(
            "rotation would exceed {MAX_LINEAGE_LENGTH} keys"
        )));
    }
    let signed = SignedSignerConfig {
        next_signer_certificate: new_public.clone(),
        algorithm: old_key.algorithm() as i32,
        metadata: Some(metadata),
    };
    let bytes = signed.encode_to_vec();
    let current = lineage.signers.last_mut().ok_or_else(|| {
        BundleError::InvalidLineage("lineage unexpectedly has no current node".to_owned())
    })?;
    current.signed_data = Some(bytes.clone());
    current.signatures = vec![old_key.sign(&bytes)];
    lineage.signers.push(SignerConfig {
        certificate: new_public,
        signed_data: None,
        signatures: Vec::new(),
    });
    verify_lineage(&lineage)?;
    Ok(lineage)
}

/// Returns true only when `old` is an exact prefix of `new` with the same root.
#[must_use]
pub fn lineage_extends(new: &VerifiedLineage, old: &VerifiedLineage) -> bool {
    new.root_key == old.root_key
        && new.chain.len() >= old.chain.len()
        && new.chain[..old.chain.len()] == old.chain
}
