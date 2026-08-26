//! Application manifest parsing.
//!
//! Parses .manifest files that declare an app's required capabilities,
//! special directory access, and metadata. The manifest format is TOML-based
//! and follows the schema defined in ADR-0030.

use crate::scp::capability::Capability;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Parsed application manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppManifest {
    pub app: AppMetadata,
    #[serde(default)]
    pub capabilities: CapabilitiesSection,
    #[serde(default)]
    pub special_directories: HashMap<String, DirectoryAccess>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilitiesSection {
    #[serde(default)]
    pub static_caps: HashMap<String, bool>,
    #[serde(default)]
    pub dynamic: HashMap<String, CapabilityRequest>,
    #[serde(default)]
    pub forbidden: HashMap<String, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityRequest {
    pub justification: String,
    #[serde(default)]
    pub optional: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectoryAccess {
    Denied,
    ReadOnly,
    ReadWrite,
}

impl AppManifest {
    /// Load a manifest from a TOML file.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ManifestError> {
        let contents =
            fs::read_to_string(path.as_ref()).map_err(|e| ManifestError::IoError(e.to_string()))?;
        Self::from_toml(&contents)
    }

    /// Parse a manifest from TOML string.
    pub fn from_toml(toml: &str) -> Result<Self, ManifestError> {
        let parsed = crate::scp::toml_parser::parse(toml)
            .map_err(|e| ManifestError::ParseError(e))?;

        // Extract [app] section
        let app_table = parsed
            .get("app")
            .and_then(|v| v.as_table())
            .ok_or_else(|| ManifestError::ParseError("[app] section is required".to_string()))?;

        let app = AppMetadata {
            id: app_table
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ManifestError::ParseError("app.id is required".to_string()))?
                .to_string(),
            name: app_table
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ManifestError::ParseError("app.name is required".to_string()))?
                .to_string(),
            version: app_table
                .get("version")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ManifestError::ParseError("app.version is required".to_string()))?
                .to_string(),
            signature: app_table.get("signature").and_then(|v| v.as_str()).map(|s| s.to_string()),
        };

        // Extract [capabilities] section (optional)
        let capabilities = if let Some(caps_table) = parsed.get("capabilities").and_then(|v| v.as_table()) {
            let mut static_caps = HashMap::new();
            if let Some(static_table) = caps_table.get("static_caps").and_then(|v| v.as_table()) {
                for (key, value) in static_table {
                    if let Some(b) = value.as_bool() {
                        static_caps.insert(key.clone(), b);
                    }
                }
            }

            let mut dynamic = HashMap::new();
            if let Some(dynamic_table) = caps_table.get("dynamic").and_then(|v| v.as_table()) {
                for (key, value) in dynamic_table {
                    if let Some(req_table) = value.as_table() {
                        let justification = req_table
                            .get("justification")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let optional = req_table
                            .get("optional")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        dynamic.insert(key.clone(), CapabilityRequest { justification, optional });
                    }
                }
            }

            let mut forbidden = HashMap::new();
            if let Some(forbidden_table) = caps_table.get("forbidden").and_then(|v| v.as_table()) {
                for (key, value) in forbidden_table {
                    if let Some(b) = value.as_bool() {
                        forbidden.insert(key.clone(), b);
                    }
                }
            }

            CapabilitiesSection {
                static_caps,
                dynamic,
                forbidden,
            }
        } else {
            CapabilitiesSection::default()
        };

        // Extract [special_directories] section (optional)
        let mut special_directories = HashMap::new();
        if let Some(dirs_table) = parsed.get("special_directories").and_then(|v| v.as_table()) {
            for (key, value) in dirs_table {
                if let Some(access_str) = value.as_str() {
                    let access = match access_str {
                        "read_write" => DirectoryAccess::ReadWrite,
                        "read_only" => DirectoryAccess::ReadOnly,
                        "denied" => DirectoryAccess::Denied,
                        _ => DirectoryAccess::Denied,
                    };
                    special_directories.insert(key.clone(), access);
                }
            }
        }

        Ok(AppManifest {
            app,
            capabilities,
            special_directories,
        })
    }

    /// Get the set of statically-declared capabilities (auto-granted at connection).
    pub fn static_capabilities(&self) -> Vec<Capability> {
        self.capabilities
            .static_caps
            .iter()
            .filter_map(|(name, &enabled)| {
                if enabled {
                    Capability::from_wire_name(name)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get capabilities that require runtime request.
    pub fn dynamic_capabilities(&self) -> Vec<(Capability, CapabilityRequest)> {
        self.capabilities
            .dynamic
            .iter()
            .filter_map(|(name, req)| {
                Capability::from_wire_name(name).map(|cap| (cap, req.clone()))
            })
            .collect()
    }

    /// Check if a capability is explicitly forbidden.
    pub fn is_forbidden(&self, capability: &Capability) -> bool {
        self.capabilities
            .forbidden
            .get(capability.wire_name())
            .copied()
            .unwrap_or(false)
    }

    /// Get special directory access level.
    pub fn directory_access(&self, directory: &str) -> DirectoryAccess {
        self.special_directories
            .get(directory)
            .copied()
            .unwrap_or(DirectoryAccess::Denied)
    }

    /// Validate the manifest structure.
    pub fn validate(&self) -> Result<(), ManifestError> {
        // Check app ID format
        if self.app.id.is_empty()
            || !self
                .app
                .id
                .chars()
                .all(|c| c.is_alphanumeric() || c == '.' || c == '-')
        {
            return Err(ManifestError::InvalidAppId(self.app.id.clone()));
        }

        // Check for conflicts between static and forbidden
        for (name, &enabled) in &self.capabilities.static_caps {
            if enabled
                && self
                    .capabilities
                    .forbidden
                    .get(name)
                    .copied()
                    .unwrap_or(false)
            {
                return Err(ManifestError::ConflictingCapabilities(name.clone()));
            }
        }

        // Check for conflicts between dynamic and forbidden
        for name in self.capabilities.dynamic.keys() {
            if self
                .capabilities
                .forbidden
                .get(name)
                .copied()
                .unwrap_or(false)
            {
                return Err(ManifestError::ConflictingCapabilities(name.clone()));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ManifestError {
    #[error("I/O error: {0}")]
    IoError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Invalid app ID: {0}")]
    InvalidAppId(String),
    #[error("Conflicting capabilities: {0}")]
    ConflictingCapabilities(String),
}

/// Example manifest for reference.
pub const EXAMPLE_MANIFEST: &str = r#"
[app]
id = "org.sol.files"
name = "SOL Files"
version = "1.0.0"
signature = "SHA256:..."

[capabilities.static_caps]
window-toplevel = true
window-popup = true

[capabilities.dynamic.notifications]
justification = "Notify file operation completion"
optional = false

[capabilities.dynamic.network_access]
justification = "Access network storage"
optional = true

[capabilities.forbidden]
screen-capture-output = true
layer-shell = true

[special_directories]
pictures = "read_write"
documents = "read_write"
downloads = "read_write"
music = "read_only"
videos = "read_only"
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_example_manifest() {
        let manifest = AppManifest::from_toml(EXAMPLE_MANIFEST).unwrap();
        assert_eq!(manifest.app.id, "org.sol.files");
        assert_eq!(manifest.app.name, "SOL Files");
        assert_eq!(manifest.app.version, "1.0.0");

        let static_caps = manifest.static_capabilities();
        assert!(static_caps.contains(&Capability::WindowToplevel));

        assert!(manifest.is_forbidden(&Capability::LayerShell));
        assert_eq!(
            manifest.directory_access("pictures"),
            DirectoryAccess::ReadWrite
        );
    }

    #[test]
    fn validates_app_id() {
        let valid = AppManifest::from_toml(
            r#"
            [app]
            id = "org.sol.example"
            name = "Test"
            version = "1.0"
        "#,
        )
        .unwrap();
        assert!(valid.validate().is_ok());

        let invalid = AppManifest::from_toml(
            r#"
            [app]
            id = "invalid id!"
            name = "Test"
            version = "1.0"
        "#,
        )
        .unwrap();
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn detects_conflicting_capabilities() {
        let manifest = AppManifest::from_toml(
            r#"
            [app]
            id = "org.example.test"
            name = "Test"
            version = "1.0"

            [capabilities.static_caps]
            clipboard_read = true

            [capabilities.forbidden]
            clipboard_read = true
        "#,
        )
        .unwrap();

        assert!(matches!(
            manifest.validate(),
            Err(ManifestError::ConflictingCapabilities(_))
        ));
    }

    #[test]
    fn handles_dynamic_capabilities() {
        let manifest = AppManifest::from_toml(
            r#"
            [app]
            id = "org.example.test"
            name = "Test"
            version = "1.0"

            [capabilities.dynamic.clipboard-read]
            justification = "Read clipboard for paste"
            optional = false

            [capabilities.dynamic.screen-capture-window]
            justification = "Take screenshots"
            optional = true
        "#,
        )
        .unwrap();

        let dynamic = manifest.dynamic_capabilities();
        assert_eq!(dynamic.len(), 2);

        let has_clipboard = dynamic
            .iter()
            .any(|(cap, _)| matches!(cap, Capability::ClipboardRead));
        assert!(has_clipboard);

        let has_screen_capture = dynamic
            .iter()
            .any(|(cap, req)| matches!(cap, Capability::ScreenCapture { .. }) && req.optional);
        assert!(has_screen_capture);
    }
}
