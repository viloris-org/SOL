use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use walkdir::WalkDir;

use crate::error::{BundleError, Result};

/// Current content-manifest format.
pub const CONTENT_MANIFEST_VERSION: u32 = 1;

/// The application identity fields sourced from `App.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppIdentity {
    /// Durable reverse-DNS application identifier.
    pub app_id: String,
    /// Human-readable release version.
    pub version: String,
    /// Monotonically increasing release number.
    pub version_code: u64,
}

/// A canonical content manifest covered by every release signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentManifest {
    /// Schema version.
    pub format_version: u32,
    /// Durable application identifier.
    pub app_id: String,
    /// Human-readable version.
    pub version: String,
    /// Monotonic anti-replay version.
    pub version_code: u64,
    /// Complete sectioned bundle inventory.
    pub bundle_sections: BundleSections,
    /// Domain-separated digest over every inventory entry.
    pub total_content_hash: String,
}

/// Content groups used by ADR-0030 and reserved forward-compatible content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleSections {
    /// Canonical application manifest.
    pub app_toml: FileRecord,
    /// Files below `bin/`.
    pub executables: FileSection,
    /// Files below `lib/`.
    pub libraries: FileSection,
    /// Files below `resources/`.
    pub resources: FileSection,
    /// SBOM, licenses, and provenance below `metadata/`.
    pub metadata: FileSection,
    /// Any other regular bundle files; these remain signature-covered.
    pub other: FileSection,
}

/// A deterministic mapping from relative paths to content bindings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileSection {
    /// Sorted entries.
    pub entries: BTreeMap<String, FileRecord>,
}

/// SHA-256 and byte length for one regular file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileRecord {
    /// Bundle-relative slash-separated path.
    pub path: String,
    /// Lowercase SHA-256 hexadecimal.
    pub sha256: String,
    /// Exact byte size.
    pub size: u64,
}

impl ContentManifest {
    /// Scans a bundle and creates its canonical complete content inventory.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid `App.toml`, unsafe bundle layout, or I/O
    /// failure while scanning content.
    pub fn build(bundle: &Path) -> Result<Self> {
        ensure_bundle_root(bundle)?;
        let app_toml_path = bundle.join("App.toml");
        let identity = read_app_identity(&app_toml_path)?;
        let inventory = scan_files(bundle)?;
        let app_toml = inventory
            .get("App.toml")
            .cloned()
            .ok_or_else(|| BundleError::InvalidLayout("App.toml is required".to_owned()))?;

        let mut sections = BundleSections {
            app_toml,
            executables: FileSection::default(),
            libraries: FileSection::default(),
            resources: FileSection::default(),
            metadata: FileSection::default(),
            other: FileSection::default(),
        };
        for (path, record) in &inventory {
            if path == "App.toml" {
                continue;
            }
            section_for_path(&mut sections, path)
                .entries
                .insert(path.clone(), record.clone());
        }
        let total_content_hash = total_content_hash(&inventory);
        Ok(Self {
            format_version: CONTENT_MANIFEST_VERSION,
            app_id: identity.app_id,
            version: identity.version,
            version_code: identity.version_code,
            bundle_sections: sections,
            total_content_hash,
        })
    }

    /// Returns canonical compact JSON bytes with a trailing newline.
    ///
    /// # Errors
    ///
    /// Returns an error if JSON serialization fails.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        let mut bytes = serde_json::to_vec(self)
            .map_err(|error| BundleError::encoding("manifest.json", error))?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    /// Parses exact canonical JSON bytes.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed, unsupported, or non-canonical JSON.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self> {
        let manifest: Self = serde_json::from_slice(bytes)
            .map_err(|error| BundleError::encoding("manifest.json", error))?;
        if manifest.format_version != CONTENT_MANIFEST_VERSION {
            return Err(BundleError::Encoding {
                kind: "manifest.json",
                message: format!("unsupported format version {}", manifest.format_version),
            });
        }
        if manifest.canonical_bytes()? != bytes {
            return Err(BundleError::Encoding {
                kind: "manifest.json",
                message: "non-canonical JSON encoding".to_owned(),
            });
        }
        Ok(manifest)
    }

    /// Re-scans a bundle and proves the inventory, identities, and total digest match.
    ///
    /// # Errors
    ///
    /// Returns an error for unsafe content or any missing, changed, or added file.
    pub fn verify_content(&self, bundle: &Path) -> Result<()> {
        let actual = Self::build(bundle)?;
        if self.app_id != actual.app_id {
            return Err(BundleError::DigestMismatch("App.toml app_id".to_owned()));
        }
        if self.version != actual.version {
            return Err(BundleError::DigestMismatch("App.toml version".to_owned()));
        }
        if self.version_code != actual.version_code {
            return Err(BundleError::DigestMismatch(
                "App.toml version_code".to_owned(),
            ));
        }

        let expected = self.inventory()?;
        let found = actual.inventory()?;
        let all_paths = expected
            .keys()
            .chain(found.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        for path in all_paths {
            if expected.get(&path) != found.get(&path) {
                return Err(BundleError::DigestMismatch(path));
            }
        }
        if self.bundle_sections != actual.bundle_sections {
            return Err(BundleError::DigestMismatch(
                "bundle section classification".to_owned(),
            ));
        }
        if self.total_content_hash != actual.total_content_hash {
            return Err(BundleError::DigestMismatch("total_content_hash".to_owned()));
        }
        Ok(())
    }

    /// Returns the decoded total content digest.
    ///
    /// # Errors
    ///
    /// Returns an error unless the stored digest is canonical SHA-256 hexadecimal.
    pub fn content_digest(&self) -> Result<Vec<u8>> {
        decode_digest(&self.total_content_hash)
    }

    fn inventory(&self) -> Result<BTreeMap<String, FileRecord>> {
        let mut inventory = BTreeMap::new();
        insert_record(&mut inventory, &self.bundle_sections.app_toml)?;
        for section in [
            &self.bundle_sections.executables,
            &self.bundle_sections.libraries,
            &self.bundle_sections.resources,
            &self.bundle_sections.metadata,
            &self.bundle_sections.other,
        ] {
            for (path, record) in &section.entries {
                if path != &record.path {
                    return Err(BundleError::InvalidLayout(format!(
                        "manifest entry key {path:?} disagrees with record path {:?}",
                        record.path
                    )));
                }
                insert_record(&mut inventory, record)?;
            }
        }
        if !inventory.contains_key("App.toml") {
            return Err(BundleError::InvalidLayout(
                "manifest does not bind App.toml".to_owned(),
            ));
        }
        Ok(inventory)
    }
}

/// Reads and validates the signed identity fields from `App.toml`.
///
/// # Errors
///
/// Returns an error when the file is unavailable, invalid TOML, or has invalid
/// required identity fields.
pub fn read_app_identity(path: &Path) -> Result<AppIdentity> {
    let bytes = fs::read(path).map_err(|error| BundleError::io(path, error))?;
    let text = std::str::from_utf8(&bytes).map_err(|error| BundleError::AppManifest {
        field: "document",
        message: error.to_string(),
    })?;
    let value = toml::from_str::<toml::Value>(text).map_err(|error| BundleError::AppManifest {
        field: "document",
        message: error.to_string(),
    })?;
    let root = value.as_table().ok_or_else(|| BundleError::AppManifest {
        field: "document",
        message: "top-level TOML value must be a table".to_owned(),
    })?;
    let table = root
        .get("app")
        .and_then(toml::Value::as_table)
        .unwrap_or(root);
    let app_id = string_field(table, "app_id")?;
    validate_app_id(&app_id)?;
    let version = string_field(table, "version")?;
    if version.trim().is_empty() || version != version.trim() {
        return Err(BundleError::AppManifest {
            field: "version",
            message: "must be non-empty without surrounding whitespace".to_owned(),
        });
    }
    let version_code = table
        .get("version_code")
        .and_then(toml::Value::as_integer)
        .and_then(|value| u64::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| BundleError::AppManifest {
            field: "version_code",
            message: "must be a positive integer".to_owned(),
        })?;
    Ok(AppIdentity {
        app_id,
        version,
        version_code,
    })
}

fn ensure_bundle_root(bundle: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(bundle).map_err(|error| BundleError::io(bundle, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(BundleError::InvalidLayout(format!(
            "{} must be a real directory",
            bundle.display()
        )));
    }
    Ok(())
}

fn scan_files(bundle: &Path) -> Result<BTreeMap<String, FileRecord>> {
    let mut inventory = BTreeMap::new();
    for entry in WalkDir::new(bundle).follow_links(false).sort_by_file_name() {
        let entry = entry.map_err(|error| {
            let path = error.path().unwrap_or(bundle).to_path_buf();
            BundleError::InvalidLayout(format!("cannot walk {}: {error}", path.display()))
        })?;
        if entry.path() == bundle {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(bundle)
            .map_err(|error| BundleError::InvalidLayout(error.to_string()))?;
        let path = canonical_relative_path(relative)?;
        if entry.file_type().is_symlink() {
            return Err(BundleError::InvalidLayout(format!(
                "symbolic links are forbidden: {path}"
            )));
        }
        if path == ".signatures" || path.starts_with(".signatures/") {
            continue;
        }
        if entry.file_type().is_dir() {
            continue;
        }
        if !entry.file_type().is_file() {
            return Err(BundleError::InvalidLayout(format!(
                "only regular files are supported: {path}"
            )));
        }
        let metadata = entry
            .metadata()
            .map_err(|error| BundleError::io(entry.path(), error.into()))?;
        if metadata.permissions().mode() & 0o111 != 0 && !path.starts_with("bin/") {
            return Err(BundleError::InvalidLayout(format!(
                "executable content must be below bin/: {path}"
            )));
        }
        let bytes = fs::read(entry.path()).map_err(|error| BundleError::io(entry.path(), error))?;
        let record = FileRecord {
            path: path.clone(),
            sha256: hex::encode(Sha256::digest(&bytes)),
            size: u64::try_from(bytes.len())
                .map_err(|_| BundleError::InvalidLayout(format!("file is too large: {path}")))?,
        };
        inventory.insert(path, record);
    }
    Ok(inventory)
}

fn canonical_relative_path(path: &Path) -> Result<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => {
                let value = value.to_str().ok_or_else(|| {
                    BundleError::InvalidLayout("bundle paths must be valid UTF-8".to_owned())
                })?;
                if value.is_empty() || value == "." || value == ".." {
                    return Err(BundleError::InvalidLayout(format!(
                        "non-canonical path component {value:?}"
                    )));
                }
                parts.push(value);
            }
            _ => {
                return Err(BundleError::InvalidLayout(format!(
                    "bundle path must be relative: {}",
                    path.display()
                )));
            }
        }
    }
    Ok(parts.join("/"))
}

fn section_for_path<'a>(sections: &'a mut BundleSections, path: &str) -> &'a mut FileSection {
    if path.starts_with("bin/") {
        &mut sections.executables
    } else if path.starts_with("lib/") {
        &mut sections.libraries
    } else if path.starts_with("resources/") {
        &mut sections.resources
    } else if path.starts_with("metadata/") {
        &mut sections.metadata
    } else {
        &mut sections.other
    }
}

fn total_content_hash(inventory: &BTreeMap<String, FileRecord>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"SOL-BUNDLE-CONTENT\0");
    for record in inventory.values() {
        let path = record.path.as_bytes();
        hasher.update(u32::try_from(path.len()).unwrap_or(u32::MAX).to_be_bytes());
        hasher.update(path);
        hasher.update(record.size.to_be_bytes());
        // Generated records always contain valid hexadecimal.
        hasher.update(hex::decode(&record.sha256).unwrap_or_default());
    }
    hex::encode(hasher.finalize())
}

fn insert_record(inventory: &mut BTreeMap<String, FileRecord>, record: &FileRecord) -> Result<()> {
    canonical_relative_path(Path::new(&record.path))?;
    decode_digest(&record.sha256)?;
    if inventory
        .insert(record.path.clone(), record.clone())
        .is_some()
    {
        return Err(BundleError::InvalidLayout(format!(
            "duplicate manifest path {}",
            record.path
        )));
    }
    Ok(())
}

fn decode_digest(value: &str) -> Result<Vec<u8>> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(BundleError::Encoding {
            kind: "SHA-256 digest",
            message: "expected 64 lowercase hexadecimal characters".to_owned(),
        });
    }
    hex::decode(value).map_err(|error| BundleError::encoding("SHA-256 digest", error))
}

fn string_field(table: &toml::Table, name: &'static str) -> Result<String> {
    table
        .get(name)
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| BundleError::AppManifest {
            field: name,
            message: "must be a string".to_owned(),
        })
}

fn validate_app_id(value: &str) -> Result<()> {
    let valid = value.len() <= 255
        && value.split('.').count() >= 3
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && segment.len() <= 63
                && segment
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_lowercase())
                && segment.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || byte == b'_'
                        || byte == b'-'
                })
        });
    if !valid {
        return Err(BundleError::AppManifest {
            field: "app_id",
            message: "must be a lowercase reverse-DNS identifier with at least three components"
                .to_owned(),
        });
    }
    Ok(())
}
