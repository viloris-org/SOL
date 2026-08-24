use sol_image::{
    ArtifactPaths, ComponentIdentity, DeploymentManifest, DeploymentSlot, DmVerityBinding,
    ManifestFormat, RuntimeDescriptor, UkiDeploymentBinding, build_manifest, build_manifest_v2,
};
use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use sol_boot_core::{
    ArtifactBinding, AttemptId, BootState, BootSuccessReport, DeploymentDescriptor, DeploymentId,
    DurableBootState, SignedDeploymentDescriptor, select_redundant_state,
};

const USAGE: &str = "\
Usage:
  sol-image manifest --slot A|B --generation N --version VERSION \\
    --kernel PATH --initrd PATH --root-image PATH \\
    [--uki PATH --kernel-component NAME:IDENTITY \\
     --initrd-component NAME:IDENTITY \\
     --dm-verity-root-hash HASH --dm-verity-slot-root IDENTITY] \\
    --runtime NAME:REVISION[:FEATURE,FEATURE] [--runtime ...] --output PATH
  sol-image verify --manifest PATH --kernel PATH --initrd PATH --root-image PATH [--uki PATH]
  sol-image boot-descriptor --slot A|B --generation N --manifest PATH --uki PATH \
    --signing-key PATH --output PATH
  sol-image release-public-key --signing-key PATH
  sol-image init-boot-state --slot A|B --generation N --state-a PATH --state-b PATH
  sol-image stage-boot-trial --slot A|B --generation N --attempts N \
    --state-a PATH --state-b PATH
  sol-image success-report --slot A|B --generation N --attempt N --output PATH

Use --uki and its companion flags to produce a UKI-aware V2 manifest.
Omit them to produce a V1 manifest (the original schema).
Verification requires --uki for V2 and rejects it for V1.";

fn main() -> ExitCode {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    match run(&arguments) {
        Ok(message) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("sol-image: {error}\n\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: &[String]) -> Result<String, String> {
    let Some((command, options)) = arguments.split_first() else {
        return Err("a command is required".to_owned());
    };
    match command.as_str() {
        "manifest" => create_manifest(options),
        "verify" => verify_manifest(options),
        "boot-descriptor" => create_boot_descriptor(options),
        "release-public-key" => release_public_key(options),
        "init-boot-state" => initialize_boot_state(options),
        "stage-boot-trial" => stage_boot_trial(options),
        "success-report" => create_success_report(options),
        "--help" | "-h" | "help" => Ok(USAGE.to_owned()),
        _ => Err(format!("unknown command {command:?}")),
    }
}

fn release_public_key(arguments: &[String]) -> Result<String, String> {
    let options = KeyedOptions::parse(arguments, &["--signing-key"])?;
    let key = read_signing_key(&PathBuf::from(options.required("--signing-key")?))?;
    Ok(hex::encode(key.verifying_key().to_bytes()))
}

fn create_boot_descriptor(arguments: &[String]) -> Result<String, String> {
    let options = KeyedOptions::parse(
        arguments,
        &[
            "--slot",
            "--generation",
            "--manifest",
            "--uki",
            "--signing-key",
            "--output",
        ],
    )?;
    let slot = parse_boot_slot(options.required("--slot")?)?;
    let generation = parse_u64("--generation", options.required("--generation")?)?;
    let manifest_path = PathBuf::from(options.required("--manifest")?);
    let uki_path = PathBuf::from(options.required("--uki")?);
    let manifest = fs::read(&manifest_path)
        .map_err(|error| format!("cannot read {}: {error}", manifest_path.display()))?;
    let decoded = DeploymentManifest::from_canonical_bytes(&manifest)
        .map_err(|error| format!("deployment manifest: {error}"))?;
    if decoded.manifest_format() != Some(ManifestFormat::V2) {
        return Err("boot descriptors require a UKI-aware format 2 manifest".to_owned());
    }
    if decoded.slot().to_string() != options.required("--slot")?.to_ascii_uppercase()
        || decoded.generation() != generation
    {
        return Err("descriptor slot/generation must match the canonical manifest".to_owned());
    }
    let uki = fs::read(&uki_path)
        .map_err(|error| format!("cannot read {}: {error}", uki_path.display()))?;
    let deployment = DeploymentId::new(slot, generation).map_err(|error| error.to_string())?;
    let descriptor = DeploymentDescriptor::new(
        deployment,
        artifact_binding(&manifest)?,
        artifact_binding(&uki)?,
    );
    let key_path = PathBuf::from(options.required("--signing-key")?);
    let key = read_signing_key(&key_path)?;
    let signature = key.sign(&descriptor.canonical_payload()).to_bytes();
    let signed = SignedDeploymentDescriptor::new(descriptor, signature);
    let output = PathBuf::from(options.required("--output")?);
    write_atomic_bytes(&output, &signed.canonical_bytes())?;
    Ok(format!(
        "wrote signed deployment descriptor {} (release public key {})",
        output.display(),
        hex::encode(key.verifying_key().to_bytes())
    ))
}

fn initialize_boot_state(arguments: &[String]) -> Result<String, String> {
    let options = KeyedOptions::parse(
        arguments,
        &["--slot", "--generation", "--state-a", "--state-b"],
    )?;
    let deployment = parse_deployment(&options)?;
    let envelope =
        DurableBootState::new(BootState::new(deployment)).map_err(|error| error.to_string())?;
    write_state_pair(&options, envelope)?;
    Ok(format!(
        "initialized redundant boot state for slot {} generation {}",
        options.required("--slot")?,
        deployment.generation()
    ))
}

fn stage_boot_trial(arguments: &[String]) -> Result<String, String> {
    let options = KeyedOptions::parse(
        arguments,
        &[
            "--slot",
            "--generation",
            "--attempts",
            "--state-a",
            "--state-b",
        ],
    )?;
    let deployment = parse_deployment(&options)?;
    let attempts = options
        .required("--attempts")?
        .parse::<u8>()
        .map_err(|error| format!("invalid --attempts: {error}"))?;
    let state_a_path = PathBuf::from(options.required("--state-a")?);
    let state_b_path = PathBuf::from(options.required("--state-b")?);
    let state_a = fs::read(&state_a_path).ok();
    let state_b = fs::read(&state_b_path).ok();
    let selected = select_redundant_state(state_a.as_deref(), state_b.as_deref())
        .map_err(|error| format!("cannot select existing boot state: {error}"))?;
    let staged = selected
        .envelope()
        .state()
        .stage_trial(deployment, attempts)
        .map_err(|error| error.to_string())?;
    let envelope = selected
        .envelope()
        .advance(staged)
        .map_err(|error| error.to_string())?;
    write_state_pair(&options, envelope)?;
    Ok(format!(
        "staged slot {} generation {} for {attempts} attempts",
        options.required("--slot")?,
        deployment.generation()
    ))
}

fn create_success_report(arguments: &[String]) -> Result<String, String> {
    let options = KeyedOptions::parse(
        arguments,
        &["--slot", "--generation", "--attempt", "--output"],
    )?;
    let report = BootSuccessReport {
        deployment: parse_deployment(&options)?,
        attempt: AttemptId::new(parse_u64("--attempt", options.required("--attempt")?)?)
            .map_err(|error| error.to_string())?,
    };
    let output = PathBuf::from(options.required("--output")?);
    write_atomic_bytes(&output, &report.canonical_bytes())?;
    Ok(format!("wrote boot success report {}", output.display()))
}

fn parse_deployment(options: &KeyedOptions) -> Result<DeploymentId, String> {
    DeploymentId::new(
        parse_boot_slot(options.required("--slot")?)?,
        parse_u64("--generation", options.required("--generation")?)?,
    )
    .map_err(|error| error.to_string())
}

fn parse_boot_slot(value: &str) -> Result<sol_boot_core::DeploymentSlot, String> {
    match value {
        "A" | "a" => Ok(sol_boot_core::DeploymentSlot::A),
        "B" | "b" => Ok(sol_boot_core::DeploymentSlot::B),
        _ => Err("--slot must be A or B".to_owned()),
    }
}

fn parse_u64(name: &str, value: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|error| format!("invalid {name}: {error}"))
}

fn artifact_binding(bytes: &[u8]) -> Result<ArtifactBinding, String> {
    Ok(ArtifactBinding::new(
        u64::try_from(bytes.len()).map_err(|_| "artifact is too large".to_owned())?,
        Sha256::digest(bytes).into(),
    ))
}

fn read_signing_key(path: &PathBuf) -> Result<SigningKey, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let key_bytes = if bytes.len() == 32 {
        bytes
    } else {
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| "signing key must be 32 raw bytes or 64 hexadecimal digits".to_owned())?
            .trim();
        hex::decode(text).map_err(|_| "signing key is not valid hexadecimal".to_owned())?
    };
    let key: [u8; 32] = key_bytes
        .try_into()
        .map_err(|_| "signing key must be exactly 32 bytes".to_owned())?;
    Ok(SigningKey::from_bytes(&key))
}

fn write_state_pair(options: &KeyedOptions, envelope: DurableBootState) -> Result<(), String> {
    let bytes = envelope.canonical_bytes();
    // Either completed write is independently bootable; the second converges the pair.
    write_atomic_bytes(&PathBuf::from(options.required("--state-a")?), &bytes)?;
    write_atomic_bytes(&PathBuf::from(options.required("--state-b")?), &bytes)
}

fn write_atomic_bytes(path: &PathBuf, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "output filename must be valid UTF-8".to_owned())?;
    let temporary = parent.join(format!(".{file_name}.tmp"));
    let mut file = fs::File::create(&temporary)
        .map_err(|error| format!("cannot create {}: {error}", temporary.display()))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("cannot persist {}: {error}", temporary.display()))?;
    fs::rename(&temporary, path)
        .map_err(|error| format!("cannot commit {}: {error}", path.display()))
}

#[derive(Debug)]
struct KeyedOptions(std::collections::BTreeMap<String, String>);

impl KeyedOptions {
    fn parse(arguments: &[String], allowed: &[&str]) -> Result<Self, String> {
        let (pairs, remainder) = arguments.as_chunks::<2>();
        if !remainder.is_empty() {
            return Err("every option requires a value".to_owned());
        }
        let mut values = std::collections::BTreeMap::new();
        for pair in pairs {
            if !allowed.contains(&pair[0].as_str()) {
                return Err(format!("unknown or misplaced option {:?}", pair[0]));
            }
            if values.insert(pair[0].clone(), pair[1].clone()).is_some() {
                return Err(format!("{} may be provided only once", pair[0]));
            }
        }
        Ok(Self(values))
    }

    fn required(&self, name: &str) -> Result<&str, String> {
        self.0
            .get(name)
            .map(String::as_str)
            .ok_or_else(|| format!("{name} is required"))
    }
}

fn create_manifest(options: &[String]) -> Result<String, String> {
    let parsed = Options::parse(options, true)?;
    let slot = DeploymentSlot::parse(Options::required("--slot", parsed.slot.as_deref())?)
        .map_err(|error| error.to_string())?;
    let generation = Options::required("--generation", parsed.generation.as_deref())?
        .parse::<u64>()
        .map_err(|error| format!("invalid --generation: {error}"))?;
    let version = Options::required("--version", parsed.version.as_deref())?;
    let output = PathBuf::from(Options::required("--output", parsed.output.as_deref())?);
    if parsed.runtimes.is_empty() {
        return Err("at least one --runtime is required".to_owned());
    }
    let runtimes = parsed
        .runtimes
        .iter()
        .map(|runtime| parse_runtime(runtime))
        .collect::<Result<Vec<_>, _>>()?;

    if parsed.uki.is_some()
        || parsed.kernel_component.is_some()
        || parsed.initrd_component.is_some()
        || parsed.dm_verity_root_hash.is_some()
        || parsed.dm_verity_slot_root.is_some()
    {
        let uki_path = PathBuf::from(Options::required("--uki", parsed.uki.as_deref())?);
        let kernel_component_str =
            Options::required("--kernel-component", parsed.kernel_component.as_deref())?;
        let initrd_component_str =
            Options::required("--initrd-component", parsed.initrd_component.as_deref())?;
        let dm_verity_root_hash = Options::required(
            "--dm-verity-root-hash",
            parsed.dm_verity_root_hash.as_deref(),
        )?;
        let dm_verity_slot_root = Options::required(
            "--dm-verity-slot-root",
            parsed.dm_verity_slot_root.as_deref(),
        )?;

        let kernel_component = parse_component_identity(kernel_component_str, "kernel_component")?;
        let initrd_component = parse_component_identity(initrd_component_str, "initrd_component")?;
        let uki_deployment = UkiDeploymentBinding::from_path(
            &uki_path,
            kernel_component,
            initrd_component,
            DmVerityBinding::new(dm_verity_root_hash, dm_verity_slot_root)
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;

        let manifest = build_manifest_v2(
            slot,
            generation,
            version,
            &parsed.artifact_paths()?,
            runtimes,
            uki_deployment,
        )
        .map_err(|error| error.to_string())?;
        manifest
            .write_atomic(&output)
            .map_err(|error| error.to_string())?;
        Ok(format!(
            "wrote canonical UKI-aware deployment manifest {}",
            output.display()
        ))
    } else {
        let manifest = build_manifest(
            slot,
            generation,
            version,
            &parsed.artifact_paths()?,
            runtimes,
        )
        .map_err(|error| error.to_string())?;
        manifest
            .write_atomic(&output)
            .map_err(|error| error.to_string())?;
        Ok(format!(
            "wrote canonical deployment manifest {}",
            output.display()
        ))
    }
}

fn verify_manifest(options: &[String]) -> Result<String, String> {
    let parsed = Options::parse(options, false)?;
    let manifest_path = PathBuf::from(Options::required("--manifest", parsed.manifest.as_deref())?);
    let bytes = fs::read(&manifest_path)
        .map_err(|error| format!("cannot read {}: {error}", manifest_path.display()))?;
    let manifest =
        DeploymentManifest::from_canonical_bytes(&bytes).map_err(|error| error.to_string())?;
    let uki_path = match manifest.manifest_format() {
        Some(ManifestFormat::V1) => {
            if parsed.uki.is_some() {
                return Err("--uki is not valid for a format 1 manifest".to_owned());
            }
            None
        }
        Some(ManifestFormat::V2) => Some(PathBuf::from(Options::required(
            "--uki",
            parsed.uki.as_deref(),
        )?)),
        None => return Err("manifest format became unsupported after decoding".to_owned()),
    };
    manifest
        .verify_artifacts(&parsed.artifact_paths()?, uki_path.as_deref())
        .map_err(|error| error.to_string())?;
    Ok(format!(
        "verified slot {} generation {} ({})",
        manifest.slot(),
        manifest.generation(),
        manifest.system_version()
    ))
}

fn parse_component_identity(value: &str, label: &'static str) -> Result<ComponentIdentity, String> {
    let mut parts = value.splitn(2, ':');
    let name = parts
        .next()
        .ok_or_else(|| format!("{label} requires NAME:IDENTITY"))?;
    let identity = parts
        .next()
        .ok_or_else(|| format!("{label} requires NAME:IDENTITY"))?;
    ComponentIdentity::new(name, identity).map_err(|error| error.to_string())
}

fn parse_runtime(value: &str) -> Result<RuntimeDescriptor, String> {
    let mut parts = value.splitn(3, ':');
    let name = parts.next().unwrap_or_default();
    let revision = parts
        .next()
        .ok_or_else(|| "--runtime requires NAME:REVISION[:FEATURE,FEATURE]".to_owned())?
        .parse::<u64>()
        .map_err(|error| format!("invalid runtime revision: {error}"))?;
    let features = parts
        .next()
        .filter(|features| !features.is_empty())
        .map_or_else(Vec::new, |features| {
            features.split(',').map(ToOwned::to_owned).collect()
        });
    RuntimeDescriptor::new(name, revision, features).map_err(|error| error.to_string())
}

#[derive(Debug, Default)]
struct Options {
    slot: Option<String>,
    generation: Option<String>,
    version: Option<String>,
    kernel: Option<String>,
    initrd: Option<String>,
    root_image: Option<String>,
    output: Option<String>,
    manifest: Option<String>,
    runtimes: Vec<String>,
    uki: Option<String>,
    kernel_component: Option<String>,
    initrd_component: Option<String>,
    dm_verity_root_hash: Option<String>,
    dm_verity_slot_root: Option<String>,
}

impl Options {
    fn parse(arguments: &[String], allow_manifest_fields: bool) -> Result<Self, String> {
        let mut options = Self::default();
        let mut index = 0;
        while index < arguments.len() {
            let name = &arguments[index];
            let value = arguments
                .get(index + 1)
                .ok_or_else(|| format!("{name} requires a value"))?
                .clone();
            let target = match name.as_str() {
                "--slot" if allow_manifest_fields => &mut options.slot,
                "--generation" if allow_manifest_fields => &mut options.generation,
                "--version" if allow_manifest_fields => &mut options.version,
                "--kernel" => &mut options.kernel,
                "--initrd" => &mut options.initrd,
                "--root-image" => &mut options.root_image,
                "--output" if allow_manifest_fields => &mut options.output,
                "--manifest" if !allow_manifest_fields => &mut options.manifest,
                "--runtime" if allow_manifest_fields => {
                    options.runtimes.push(value);
                    index += 2;
                    continue;
                }
                "--uki" => &mut options.uki,
                "--kernel-component" if allow_manifest_fields => &mut options.kernel_component,
                "--initrd-component" if allow_manifest_fields => &mut options.initrd_component,
                "--dm-verity-root-hash" if allow_manifest_fields => {
                    &mut options.dm_verity_root_hash
                }
                "--dm-verity-slot-root" if allow_manifest_fields => {
                    &mut options.dm_verity_slot_root
                }
                _ => return Err(format!("unknown or misplaced option {name:?}")),
            };
            if target.replace(value).is_some() {
                return Err(format!("{name} may be provided only once"));
            }
            index += 2;
        }
        Ok(options)
    }

    fn required<'a>(name: &str, value: Option<&'a str>) -> Result<&'a str, String> {
        value.ok_or_else(|| format!("{name} is required"))
    }

    fn artifact_paths(&self) -> Result<ArtifactPaths, String> {
        Ok(ArtifactPaths {
            kernel: PathBuf::from(Self::required("--kernel", self.kernel.as_deref())?),
            initrd: PathBuf::from(Self::required("--initrd", self.initrd.as_deref())?),
            root_image: PathBuf::from(Self::required("--root-image", self.root_image.as_deref())?),
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn runtime_argument_supports_sorted_feature_canonicalization() {
        let runtime = parse_runtime("sol-runtime-1:12:documents.v2,accessibility.tree-v1")
            .expect("runtime argument");
        assert_eq!(runtime.contract_revision(), 12);
        assert_eq!(
            runtime.features(),
            &[
                "accessibility.tree-v1".to_owned(),
                "documents.v2".to_owned()
            ]
        );
    }

    #[test]
    fn duplicate_scalar_option_is_rejected() {
        let error = Options::parse(
            &[
                "--slot".to_owned(),
                "A".to_owned(),
                "--slot".to_owned(),
                "B".to_owned(),
            ],
            true,
        )
        .expect_err("duplicate must fail");
        assert!(error.contains("only once"));
    }
}
