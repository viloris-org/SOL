use sol_image::{
    ArtifactPaths, ComponentIdentity, DeploymentManifest, DeploymentSlot, DmVerityBinding,
    ManifestFormat, RuntimeDescriptor, UkiDeploymentBinding, build_manifest, build_manifest_v2,
};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
Usage:
  sol-image manifest --slot A|B --generation N --version VERSION \\
    --kernel PATH --initrd PATH --root-image PATH \\
    [--uki PATH --kernel-component NAME:IDENTITY \\
     --initrd-component NAME:IDENTITY \\
     --dm-verity-root-hash HASH --dm-verity-slot-root IDENTITY] \\
    --runtime NAME:REVISION[:FEATURE,FEATURE] [--runtime ...] --output PATH
  sol-image verify --manifest PATH --kernel PATH --initrd PATH --root-image PATH [--uki PATH]

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
        "--help" | "-h" | "help" => Ok(USAGE.to_owned()),
        _ => Err(format!("unknown command {command:?}")),
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
