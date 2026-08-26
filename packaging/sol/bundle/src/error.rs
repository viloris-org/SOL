use std::path::PathBuf;

/// An invalid bundle, key, signature, or filesystem operation.
#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    /// An I/O operation failed.
    #[error("I/O error at {path}: {source}")]
    Io {
        /// Path involved in the operation.
        path: PathBuf,
        /// Underlying error.
        source: std::io::Error,
    },
    /// A serialized object could not be decoded or encoded.
    #[error("invalid {kind}: {message}")]
    Encoding {
        /// Object being processed.
        kind: &'static str,
        /// Codec error details.
        message: String,
    },
    /// The app manifest is missing a required or valid field.
    #[error("invalid App.toml field {field}: {message}")]
    AppManifest {
        /// TOML field name.
        field: &'static str,
        /// Validation failure.
        message: String,
    },
    /// The bundle layout is unsafe or ambiguous.
    #[error("invalid bundle layout: {0}")]
    InvalidLayout(String),
    /// A content binding does not match.
    #[error("digest mismatch in {0}")]
    DigestMismatch(String),
    /// A signed field disagrees with the content manifest.
    #[error("signed {0} does not match manifest.json")]
    SignedDataMismatch(&'static str),
    /// The signature block contains no signer.
    #[error("signature block contains no signers")]
    NoSigners,
    /// A signer has no signature.
    #[error("signer {0} contains no signatures")]
    NoSignatures(usize),
    /// A cryptographic operation failed.
    #[error("signature verification failed for signer {signer}: {message}")]
    InvalidSignature {
        /// Signer index.
        signer: usize,
        /// Verification details.
        message: String,
    },
    /// The requested algorithm is unknown or disabled.
    #[error("unsupported signature algorithm {0}")]
    UnsupportedAlgorithm(i32),
    /// A private or public key has the wrong format.
    #[error("invalid {algorithm} key: {message}")]
    InvalidKey {
        /// Algorithm label.
        algorithm: &'static str,
        /// Parser error.
        message: String,
    },
    /// The lineage protobuf is malformed or cryptographically invalid.
    #[error("invalid publisher lineage: {0}")]
    InvalidLineage(String),
    /// The signing key is not the current lineage key.
    #[error("signing key does not match the lineage current key")]
    SignerLineageMismatch,
    /// A version is not strictly newer than the installed version.
    #[error("downgrade/replay rejected: installed version_code {installed}, candidate {candidate}")]
    DowngradeAttempt {
        /// Installed monotonic version.
        installed: u64,
        /// Candidate monotonic version.
        candidate: u64,
    },
    /// A revocation entry blocks the signing key.
    #[error("signing key {fingerprint} was revoked: {reason}")]
    KeyRevoked {
        /// SHA-256 public-key fingerprint.
        fingerprint: String,
        /// Repository-provided reason.
        reason: String,
    },
}

impl BundleError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub(crate) fn encoding(kind: &'static str, error: impl std::fmt::Display) -> Self {
        Self::Encoding {
            kind,
            message: error.to_string(),
        }
    }
}

/// Result type used by the bundle signing library.
pub type Result<T> = std::result::Result<T, BundleError>;
