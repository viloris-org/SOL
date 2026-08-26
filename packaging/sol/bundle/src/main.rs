use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use sol_bundle::{
    CacheState, GrantInheritance, PrivateKey, RevocationCache, RotationMetadata,
    SignatureAlgorithm, add_signer, check_update, fingerprint, generate_key, read_lineage,
    rotate_lineage, sign_bundle, unix_timestamp_now, verify_app_bundle, write_lineage,
};

#[derive(Debug, Parser)]
#[command(
    name = "sol-bundle",
    version,
    about = "Sign and verify SOL .app bundles"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Generate a publisher signing key in PKCS#8 PEM format.
    Keygen {
        /// Signing algorithm.
        #[arg(long, value_enum, default_value_t = Algorithm::Ed25519)]
        algorithm: Algorithm,
        /// New key path; existing files are never overwritten.
        #[arg(long)]
        out: PathBuf,
    },
    /// Replace a bundle's signature block with one signer.
    Sign {
        /// `.app` directory.
        bundle: PathBuf,
        /// PKCS#8 PEM private key.
        #[arg(long)]
        key: PathBuf,
        /// Private-key algorithm.
        #[arg(long, value_enum, default_value_t = Algorithm::Ed25519)]
        algorithm: Algorithm,
        /// Optional publisher lineage protobuf.
        #[arg(long)]
        lineage: Option<PathBuf>,
        /// Minimum SOL version accepted by this bundle.
        #[arg(long, default_value_t = 1)]
        min_sol_version: u32,
        /// Reproducible UTC Unix timestamp; defaults to the current time.
        #[arg(long)]
        timestamp: Option<i64>,
    },
    /// Append an independent signer; all signers remain mandatory.
    AddSigner {
        /// Signed `.app` directory.
        bundle: PathBuf,
        /// PKCS#8 PEM private key.
        #[arg(long)]
        key: PathBuf,
        /// Private-key algorithm.
        #[arg(long, value_enum, default_value_t = Algorithm::Ed25519)]
        algorithm: Algorithm,
        /// Optional publisher lineage protobuf.
        #[arg(long)]
        lineage: Option<PathBuf>,
        /// Reproducible UTC Unix timestamp; defaults to the current time.
        #[arg(long)]
        timestamp: Option<i64>,
    },
    /// Verify all bundle content, signers, and lineages.
    Verify {
        /// Signed `.app` directory.
        bundle: PathBuf,
        /// Optional JSON revocation cache.
        #[arg(long)]
        revocation_cache: Option<PathBuf>,
        /// Print the primary key-rotation history.
        #[arg(long)]
        show_lineage: bool,
    },
    /// Authorize a new publisher key with the current key.
    RotateKey {
        /// Current private key.
        #[arg(long)]
        old_key: PathBuf,
        /// New private key.
        #[arg(long)]
        new_key: PathBuf,
        /// Current-key algorithm.
        #[arg(long, value_enum, default_value_t = Algorithm::Ed25519)]
        old_algorithm: Algorithm,
        /// New-key algorithm.
        #[arg(long, value_enum, default_value_t = Algorithm::Ed25519)]
        new_algorithm: Algorithm,
        /// Existing lineage to extend; omit for the first rotation.
        #[arg(long)]
        lineage: Option<PathBuf>,
        /// Stable signature-covered reason, such as `key_expiry`.
        #[arg(long)]
        reason: String,
        /// Signature-covered human-readable context.
        #[arg(long, default_value = "")]
        description: String,
        /// Reproducible UTC Unix timestamp; defaults to the current time.
        #[arg(long)]
        timestamp: Option<i64>,
        /// Output lineage protobuf.
        #[arg(long)]
        out: PathBuf,
    },
    /// Verify an installed and candidate bundle and evaluate grant continuity.
    CheckInheritance {
        /// Currently installed `.app` directory.
        installed: PathBuf,
        /// Candidate update `.app` directory.
        candidate: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Algorithm {
    Ed25519,
    EcdsaP256,
    Rsa4096,
}

impl From<Algorithm> for SignatureAlgorithm {
    fn from(value: Algorithm) -> Self {
        match value {
            Algorithm::Ed25519 => Self::Ed25519,
            Algorithm::EcdsaP256 => Self::EcdsaP256Sha256,
            Algorithm::Rsa4096 => Self::Rsa4096Sha256,
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(message) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("sol-bundle: {error}");
            ExitCode::FAILURE
        }
    }
}

#[allow(clippy::too_many_lines)]
fn run(cli: Cli) -> Result<String, Box<dyn std::error::Error>> {
    match cli.command {
        Command::Keygen { algorithm, out } => {
            let fingerprint = generate_key(&out, algorithm.into())?;
            Ok(format!(
                "generated {} publisher key {}\nfingerprint: {fingerprint}",
                algorithm_name(algorithm),
                out.display()
            ))
        }
        Command::Sign {
            bundle,
            key,
            algorithm,
            lineage,
            min_sol_version,
            timestamp,
        } => {
            let key = PrivateKey::read(&key, algorithm.into())?;
            let lineage = lineage.as_deref().map(read_lineage).transpose()?;
            let identity = sign_bundle(
                &bundle,
                &key,
                lineage.as_ref(),
                min_sol_version,
                timestamp.unwrap_or(unix_timestamp_now()?),
            )?;
            Ok(format_identity("signed", &bundle, &identity))
        }
        Command::AddSigner {
            bundle,
            key,
            algorithm,
            lineage,
            timestamp,
        } => {
            let key = PrivateKey::read(&key, algorithm.into())?;
            let lineage = lineage.as_deref().map(read_lineage).transpose()?;
            let identity = add_signer(
                &bundle,
                &key,
                lineage.as_ref(),
                timestamp.unwrap_or(unix_timestamp_now()?),
            )?;
            Ok(format_identity("added signer to", &bundle, &identity))
        }
        Command::Verify {
            bundle,
            revocation_cache,
            show_lineage,
        } => {
            let cache = revocation_cache
                .as_deref()
                .map(read_revocation_cache)
                .transpose()?;
            let identity = verify_app_bundle(&bundle, cache.as_ref())?;
            let mut output = format_identity("verified", &bundle, &identity);
            if let Some(cache) = &cache {
                let state = cache.state_at(unix_timestamp_now()?);
                write!(output, "\nrevocation cache: {}", cache_state_name(state))?;
            }
            if show_lineage {
                for (index, key) in identity.publisher_lineage.chain.iter().enumerate() {
                    write!(output, "\nlineage[{index}]: {}", fingerprint(key))?;
                    if let Some(rotation) = identity.publisher_lineage.rotations.get(index) {
                        write!(
                            output,
                            " -> {} at {} ({})",
                            rotation.reason, rotation.timestamp, rotation.description
                        )?;
                    }
                }
            }
            Ok(output)
        }
        Command::RotateKey {
            old_key,
            new_key,
            old_algorithm,
            new_algorithm,
            lineage,
            reason,
            description,
            timestamp,
            out,
        } => {
            if reason.trim().is_empty() || reason != reason.trim() {
                return Err("--reason must be non-empty without surrounding whitespace".into());
            }
            let old_key = PrivateKey::read(&old_key, old_algorithm.into())?;
            let new_key = PrivateKey::read(&new_key, new_algorithm.into())?;
            let existing = lineage.as_deref().map(read_lineage).transpose()?;
            let lineage = rotate_lineage(
                existing,
                &old_key,
                &new_key,
                RotationMetadata {
                    reason,
                    timestamp: timestamp.unwrap_or(unix_timestamp_now()?),
                    description,
                },
            )?;
            write_lineage(&out, &lineage)?;
            Ok(format!(
                "created lineage {} with {} keys\nroot: {}\ncurrent: {}",
                out.display(),
                lineage.signers.len(),
                fingerprint(&lineage.signers[0].certificate),
                fingerprint(&lineage.signers[lineage.signers.len() - 1].certificate)
            ))
        }
        Command::CheckInheritance {
            installed,
            candidate,
        } => {
            let old = verify_app_bundle(&installed, None)?;
            let new = verify_app_bundle(&candidate, None)?;
            match check_update(&old, &new)? {
                GrantInheritance::SameLineage {
                    old_root,
                    new_current,
                } => Ok(format!(
                    "same lineage; durable grants may be inherited\nroot: {old_root}\ncurrent: {new_current}"
                )),
                GrantInheritance::Discontinuous => {
                    Ok("publisher discontinuity; no grants may be inherited".to_owned())
                }
            }
        }
    }
}

fn read_revocation_cache(
    path: &std::path::Path,
) -> Result<RevocationCache, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn format_identity(
    verb: &str,
    bundle: &std::path::Path,
    identity: &sol_bundle::VerifiedIdentity,
) -> String {
    format!(
        "{verb} {}\napp_id: {}\nversion: {} ({})\nsigners: {}\npublisher root: {}\ncurrent key: {}\nbundle hash: {}\nsigned at: {}",
        bundle.display(),
        identity.app_id,
        identity.version,
        identity.version_code,
        identity.all_signers.len(),
        fingerprint(&identity.publisher_lineage.root_key),
        fingerprint(&identity.publisher_lineage.current_key),
        identity.bundle_hash,
        identity.signed_at
    )
}

const fn algorithm_name(algorithm: Algorithm) -> &'static str {
    match algorithm {
        Algorithm::Ed25519 => "Ed25519",
        Algorithm::EcdsaP256 => "ECDSA P-256",
        Algorithm::Rsa4096 => "RSA-4096",
    }
}

const fn cache_state_name(state: CacheState) -> &'static str {
    match state {
        CacheState::Fresh => "fresh",
        CacheState::Stale => "stale (24-48 hours)",
        CacheState::Expired => "expired (>48 hours or clock skew)",
        CacheState::Missing => "missing",
    }
}
