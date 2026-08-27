use prost::{Enumeration, Message};

/// On-disk SOL application signature block.
#[derive(Clone, PartialEq, Eq, Message)]
pub struct SolSignatureBlock {
    /// Every signer must verify successfully.
    #[prost(message, repeated, tag = "1")]
    pub signers: Vec<Signer>,
    /// Oldest SOL release allowed to consume this bundle.
    #[prost(uint32, tag = "2")]
    pub min_sol_version: u32,
}

/// One release signer.
#[derive(Clone, PartialEq, Eq, Message)]
pub struct Signer {
    /// Canonically encoded [`SignedData`].
    #[prost(bytes = "vec", tag = "1")]
    pub signed_data: Vec<u8>,
    /// All declared signatures must verify.
    #[prost(message, repeated, tag = "2")]
    pub signatures: Vec<Signature>,
    /// Algorithm-specific raw public key.
    #[prost(bytes = "vec", tag = "3")]
    pub public_key: Vec<u8>,
    /// Optional DER X.509 certificate for display metadata.
    #[prost(bytes = "vec", optional, tag = "4")]
    pub certificate: Option<Vec<u8>>,
}

/// Identity and digest fields covered by a release signature.
#[derive(Clone, PartialEq, Eq, Message)]
pub struct SignedData {
    /// Durable application identifier.
    #[prost(string, tag = "1")]
    pub app_id: String,
    /// Human-readable release version.
    #[prost(string, tag = "2")]
    pub version: String,
    /// Monotonic anti-replay version.
    #[prost(uint64, tag = "3")]
    pub version_code: u64,
    /// SHA-256 of exact `manifest.json` bytes.
    #[prost(bytes = "vec", tag = "4")]
    pub manifest_digest: Vec<u8>,
    /// Manifest total content digest.
    #[prost(bytes = "vec", tag = "5")]
    pub content_digest: Vec<u8>,
    /// Unix epoch seconds in UTC.
    #[prost(int64, tag = "6")]
    pub timestamp: i64,
    /// Reserved algorithm-agility digests.
    #[prost(message, repeated, tag = "7")]
    pub additional_digests: Vec<Digest>,
    /// Signature-covered copy of the outer block compatibility floor.
    #[prost(uint32, tag = "8")]
    pub min_sol_version: u32,
}

/// An optional additional digest.
#[derive(Clone, PartialEq, Eq, Message)]
pub struct Digest {
    /// Stable digest algorithm identifier.
    #[prost(uint32, tag = "1")]
    pub algorithm: u32,
    /// Raw digest bytes.
    #[prost(bytes = "vec", tag = "2")]
    pub value: Vec<u8>,
}

/// A cryptographic signature and its algorithm.
#[derive(Clone, PartialEq, Eq, Message)]
pub struct Signature {
    /// [`SignatureAlgorithm`] numeric value.
    #[prost(enumeration = "SignatureAlgorithm", tag = "1")]
    pub algorithm: i32,
    /// Raw signature bytes.
    #[prost(bytes = "vec", tag = "2")]
    pub value: Vec<u8>,
}

/// Signature algorithms assigned by ADR-0030.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Enumeration)]
#[repr(i32)]
pub enum SignatureAlgorithm {
    /// Ed25519, the preferred algorithm.
    Ed25519 = 1,
    /// ECDSA over NIST P-256 with SHA-256.
    EcdsaP256Sha256 = 2,
    /// RSA PKCS#1 v1.5 with a 4096-bit key and SHA-256.
    Rsa4096Sha256 = 3,
}

/// Publisher proof-of-key-rotation chain.
#[derive(Clone, PartialEq, Eq, Message)]
pub struct PublisherLineage {
    /// Ordered root-to-current signer configurations.
    #[prost(message, repeated, tag = "1")]
    pub signers: Vec<SignerConfig>,
    /// Lineage format version.
    #[prost(uint32, tag = "2")]
    pub version: u32,
}

/// One key in a publisher lineage.
#[derive(Clone, PartialEq, Eq, Message)]
pub struct SignerConfig {
    /// X.509 certificate or algorithm-specific raw public key.
    #[prost(bytes = "vec", tag = "1")]
    pub certificate: Vec<u8>,
    /// Encoded [`SignedSignerConfig`], absent at the current node.
    #[prost(bytes = "vec", optional, tag = "2")]
    pub signed_data: Option<Vec<u8>>,
    /// Predecessor signatures over `signed_data`.
    #[prost(message, repeated, tag = "3")]
    pub signatures: Vec<Signature>,
}

/// Rotation transition signed by the predecessor key.
#[derive(Clone, PartialEq, Eq, Message)]
pub struct SignedSignerConfig {
    /// Exact public key/certificate of the next node.
    #[prost(bytes = "vec", tag = "1")]
    pub next_signer_certificate: Vec<u8>,
    /// Algorithm used by the predecessor to sign this transition.
    #[prost(enumeration = "SignatureAlgorithm", tag = "2")]
    pub algorithm: i32,
    /// Human-readable, signature-covered rotation context.
    #[prost(message, optional, tag = "3")]
    pub metadata: Option<RotationMetadata>,
}

/// Signature-covered key rotation context.
#[derive(Clone, PartialEq, Eq, Message)]
pub struct RotationMetadata {
    /// Stable reason such as `key_expiry`.
    #[prost(string, tag = "1")]
    pub reason: String,
    /// Unix epoch seconds in UTC.
    #[prost(int64, tag = "2")]
    pub timestamp: i64,
    /// Human-readable explanation.
    #[prost(string, tag = "3")]
    pub description: String,
}
