use std::fs;
use std::path::Path;

use ed25519_dalek::pkcs8::{DecodePrivateKey as _, EncodePrivateKey as _};
use ed25519_dalek::{Signer as _, SigningKey as Ed25519SigningKey, Verifier as _};
use p256::ecdsa::{Signature as P256Signature, SigningKey as P256SigningKey};
use pkcs8::LineEnding;
use rand_core::OsRng;
use rsa::signature::SignatureEncoding as _;
use rsa::traits::PublicKeyParts as _;
use rsa::{RsaPrivateKey, RsaPublicKey};
use sha2::{Digest as _, Sha256};

use crate::error::{BundleError, Result};
use crate::proto::{Signature, SignatureAlgorithm};

/// A private publisher key loaded for an explicit algorithm.
pub enum PrivateKey {
    /// Ed25519 PKCS#8 key.
    Ed25519(Ed25519SigningKey),
    /// ECDSA P-256 PKCS#8 key.
    EcdsaP256(P256SigningKey),
    /// RSA-4096 PKCS#8 key.
    Rsa4096(Box<RsaPrivateKey>),
}

impl PrivateKey {
    /// Loads a PKCS#8 PEM key and verifies it matches `algorithm`.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be read, decoded, or does not meet
    /// the required algorithm strength.
    pub fn read(path: &Path, algorithm: SignatureAlgorithm) -> Result<Self> {
        let pem = fs::read_to_string(path).map_err(|error| BundleError::io(path, error))?;
        match algorithm {
            SignatureAlgorithm::Ed25519 => Ed25519SigningKey::from_pkcs8_pem(&pem)
                .map(Self::Ed25519)
                .map_err(|error| invalid_key("Ed25519", error)),
            SignatureAlgorithm::EcdsaP256Sha256 => P256SigningKey::from_pkcs8_pem(&pem)
                .map(Self::EcdsaP256)
                .map_err(|error| invalid_key("ECDSA P-256", error)),
            SignatureAlgorithm::Rsa4096Sha256 => {
                let key = RsaPrivateKey::from_pkcs8_pem(&pem)
                    .map_err(|error| invalid_key("RSA-4096", error))?;
                if key.size() != 512 {
                    return Err(BundleError::InvalidKey {
                        algorithm: "RSA-4096",
                        message: format!("expected a 4096-bit key, found {} bits", key.size() * 8),
                    });
                }
                Ok(Self::Rsa4096(Box::new(key)))
            }
        }
    }

    /// Generates a private key using the operating system CSPRNG.
    ///
    /// # Errors
    ///
    /// Returns an error if operating-system randomness or RSA generation fails.
    pub fn generate(algorithm: SignatureAlgorithm) -> Result<Self> {
        match algorithm {
            SignatureAlgorithm::Ed25519 => {
                Ok(Self::Ed25519(Ed25519SigningKey::generate(&mut OsRng)))
            }
            SignatureAlgorithm::EcdsaP256Sha256 => {
                Ok(Self::EcdsaP256(P256SigningKey::random(&mut OsRng)))
            }
            SignatureAlgorithm::Rsa4096Sha256 => RsaPrivateKey::new(&mut OsRng, 4096)
                .map(|key| Self::Rsa4096(Box::new(key)))
                .map_err(|error| invalid_key("RSA-4096", error)),
        }
    }

    /// Writes a PKCS#8 PEM key with owner-only permissions on Unix.
    ///
    /// # Errors
    ///
    /// Returns an error on encoding or filesystem failure, including when the
    /// output path already exists.
    pub fn write(&self, path: &Path) -> Result<()> {
        let pem = match self {
            Self::Ed25519(key) => key
                .to_pkcs8_pem(LineEnding::LF)
                .map_err(|error| invalid_key("Ed25519", error))?
                .to_string(),
            Self::EcdsaP256(key) => key
                .to_pkcs8_pem(LineEnding::LF)
                .map_err(|error| invalid_key("ECDSA P-256", error))?
                .to_string(),
            Self::Rsa4096(key) => key
                .to_pkcs8_pem(LineEnding::LF)
                .map_err(|error| invalid_key("RSA-4096", error))?
                .to_string(),
        };
        crate::write_private_file(path, pem.as_bytes())
    }

    /// Returns the ADR algorithm identifier.
    #[must_use]
    pub const fn algorithm(&self) -> SignatureAlgorithm {
        match self {
            Self::Ed25519(_) => SignatureAlgorithm::Ed25519,
            Self::EcdsaP256(_) => SignatureAlgorithm::EcdsaP256Sha256,
            Self::Rsa4096(_) => SignatureAlgorithm::Rsa4096Sha256,
        }
    }

    /// Returns the algorithm-specific public key representation stored on disk.
    ///
    /// # Errors
    ///
    /// Returns an error if an RSA public key cannot be encoded as SPKI DER.
    pub fn public_key(&self) -> Result<Vec<u8>> {
        match self {
            Self::Ed25519(key) => Ok(key.verifying_key().to_bytes().to_vec()),
            Self::EcdsaP256(key) => Ok(key
                .verifying_key()
                .to_encoded_point(false)
                .as_bytes()
                .to_vec()),
            Self::Rsa4096(key) => {
                use rsa::pkcs8::EncodePublicKey as _;
                RsaPublicKey::from(key.as_ref())
                    .to_public_key_der()
                    .map(|der| der.as_bytes().to_vec())
                    .map_err(|error| invalid_key("RSA-4096", error))
            }
        }
    }

    /// Signs exact canonical bytes.
    #[must_use]
    pub fn sign(&self, message: &[u8]) -> Signature {
        let value = match self {
            Self::Ed25519(key) => key.sign(message).to_bytes().to_vec(),
            Self::EcdsaP256(key) => {
                let signature: P256Signature = key.sign(message);
                signature.to_bytes().to_vec()
            }
            Self::Rsa4096(key) => {
                let signing_key = rsa::pkcs1v15::SigningKey::<Sha256>::new(key.as_ref().clone());
                signing_key.sign(message).to_vec()
            }
        };
        Signature {
            algorithm: self.algorithm() as i32,
            value,
        }
    }
}

/// Verifies a signature against an algorithm-specific public key.
pub fn verify(public_key: &[u8], signature: &Signature, message: &[u8]) -> Result<()> {
    let algorithm = SignatureAlgorithm::try_from(signature.algorithm)
        .map_err(|_| BundleError::UnsupportedAlgorithm(signature.algorithm))?;
    match algorithm {
        SignatureAlgorithm::Ed25519 => {
            let bytes: [u8; 32] = public_key.try_into().map_err(|_| BundleError::InvalidKey {
                algorithm: "Ed25519",
                message: "public key must be exactly 32 bytes".to_owned(),
            })?;
            let key = ed25519_dalek::VerifyingKey::from_bytes(&bytes)
                .map_err(|error| invalid_key("Ed25519", error))?;
            let signature = ed25519_dalek::Signature::from_slice(&signature.value)
                .map_err(|error| invalid_key("Ed25519 signature", error))?;
            key.verify(message, &signature)
                .map_err(|error| invalid_key("Ed25519 signature", error))
        }
        SignatureAlgorithm::EcdsaP256Sha256 => {
            let key = p256::ecdsa::VerifyingKey::from_sec1_bytes(public_key)
                .map_err(|error| invalid_key("ECDSA P-256", error))?;
            let signature = P256Signature::from_slice(&signature.value)
                .map_err(|error| invalid_key("ECDSA P-256 signature", error))?;
            key.verify(message, &signature)
                .map_err(|error| invalid_key("ECDSA P-256 signature", error))
        }
        SignatureAlgorithm::Rsa4096Sha256 => {
            use rsa::pkcs8::DecodePublicKey as _;
            let key = RsaPublicKey::from_public_key_der(public_key)
                .map_err(|error| invalid_key("RSA-4096", error))?;
            if key.size() != 512 {
                return Err(BundleError::InvalidKey {
                    algorithm: "RSA-4096",
                    message: format!("expected a 4096-bit key, found {} bits", key.size() * 8),
                });
            }
            let signature = rsa::pkcs1v15::Signature::try_from(signature.value.as_slice())
                .map_err(|error| invalid_key("RSA-4096 signature", error))?;
            rsa::pkcs1v15::VerifyingKey::<Sha256>::new(key)
                .verify(message, &signature)
                .map_err(|error| invalid_key("RSA-4096 signature", error))
        }
    }
}

/// Computes a canonical SHA-256 fingerprint of public-key bytes.
#[must_use]
pub fn fingerprint(public_key: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(public_key)))
}

fn invalid_key(algorithm: &'static str, error: impl std::fmt::Display) -> BundleError {
    BundleError::InvalidKey {
        algorithm,
        message: error.to_string(),
    }
}
