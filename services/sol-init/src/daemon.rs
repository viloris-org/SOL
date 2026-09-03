use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct DaemonDefinition {
    #[serde(rename = "Daemon", alias = "daemon")]
    pub daemon: DaemonConfig,
    #[serde(rename = "Environment", alias = "environment", default)]
    pub environment: HashMap<String, String>,
    #[serde(rename = "Resources", alias = "resources", default)]
    pub resources: ResourceConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DaemonConfig {
    pub name: String,
    pub exec: String,
    #[serde(rename = "type")]
    pub daemon_type: DaemonType,
    pub start_mode: StartMode,
    pub restart_policy: RestartPolicy,
    #[serde(default)]
    pub after: Vec<String>,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub dbus_name: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DaemonType {
    Core,
    System,
    Application,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StartMode {
    Boot,
    Dbus,
    Socket,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RestartPolicy {
    Always,
    OnFailure,
    Never,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ResourceConfig {
    pub memory_limit: Option<String>,
    pub cpu_share: Option<u32>,
}

impl DaemonDefinition {
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read daemon file: {}", path.display()))?;

        let daemon: DaemonDefinition = toml::from_str(&content)
            .with_context(|| format!("Failed to parse daemon file: {}", path.display()))?;

        Ok(daemon)
    }

    pub fn load_from_dir(dir: &Path) -> Result<HashMap<String, DaemonDefinition>> {
        let mut daemons = HashMap::new();

        if !dir.exists() {
            return Ok(daemons);
        }

        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("daemon") {
                let daemon = Self::load_from_file(&path)?;
                let name = daemon.daemon.name.clone();
                daemons.insert(name, daemon);
            }
        }

        Ok(daemons)
    }
}

/// Topological sort for daemon dependencies
pub fn topological_sort(daemons: &HashMap<String, DaemonDefinition>) -> Result<Vec<String>> {
    let mut sorted = Vec::new();
    let mut visited = HashMap::new();
    let mut temp_mark = HashMap::new();

    for name in daemons.keys() {
        if !visited.contains_key(name) {
            visit(name, daemons, &mut visited, &mut temp_mark, &mut sorted)?;
        }
    }

    Ok(sorted)
}

fn visit(
    name: &str,
    daemons: &HashMap<String, DaemonDefinition>,
    visited: &mut HashMap<String, bool>,
    temp_mark: &mut HashMap<String, bool>,
    sorted: &mut Vec<String>,
) -> Result<()> {
    if temp_mark.contains_key(name) {
        anyhow::bail!("Circular dependency detected involving daemon: {}", name);
    }

    if visited.contains_key(name) {
        return Ok(());
    }

    temp_mark.insert(name.to_string(), true);

    if let Some(daemon) = daemons.get(name) {
        for dep in &daemon.daemon.after {
            if daemons.contains_key(dep) {
                visit(dep, daemons, visited, temp_mark, sorted)?;
            }
        }

        for dep in &daemon.daemon.requires {
            if daemons.contains_key(dep) {
                visit(dep, daemons, visited, temp_mark, sorted)?;
            }
        }
    }

    temp_mark.remove(name);
    visited.insert(name.to_string(), true);
    sorted.push(name.to_string());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topological_sort() {
        let mut daemons = HashMap::new();

        daemons.insert(
            "compositor".to_string(),
            DaemonDefinition {
                daemon: DaemonConfig {
                    name: "compositor".to_string(),
                    exec: "/usr/bin/sol-compositor".to_string(),
                    daemon_type: DaemonType::Core,
                    start_mode: StartMode::Boot,
                    restart_policy: RestartPolicy::Always,
                    after: vec![],
                    requires: vec![],
                    capabilities: vec![],
                    dbus_name: None,
                },
                environment: HashMap::new(),
                resources: ResourceConfig::default(),
            },
        );

        daemons.insert(
            "shell".to_string(),
            DaemonDefinition {
                daemon: DaemonConfig {
                    name: "shell".to_string(),
                    exec: "/usr/bin/sol-shell".to_string(),
                    daemon_type: DaemonType::Core,
                    start_mode: StartMode::Boot,
                    restart_policy: RestartPolicy::Always,
                    after: vec!["compositor".to_string()],
                    requires: vec!["compositor".to_string()],
                    capabilities: vec![],
                    dbus_name: None,
                },
                environment: HashMap::new(),
                resources: ResourceConfig::default(),
            },
        );

        let order = topological_sort(&daemons).unwrap();

        let compositor_idx = order.iter().position(|n| n == "compositor").unwrap();
        let shell_idx = order.iter().position(|n| n == "shell").unwrap();

        assert!(
            compositor_idx < shell_idx,
            "compositor must start before shell"
        );
    }

    #[test]
    fn scheduling_daemon_definitions_are_valid() {
        for (name, definition) in [
            ("sol-audio", include_str!("../daemons/sol-audio.daemon")),
            (
                "sol-networkd",
                include_str!("../daemons/sol-networkd.daemon"),
            ),
            ("sol-portal", include_str!("../daemons/sol-portal.daemon")),
            ("sol-logind", include_str!("../daemons/sol-logind.daemon")),
        ] {
            assert!(
                toml::from_str::<DaemonDefinition>(definition).is_ok(),
                "invalid daemon definition: {name}"
            );
        }
    }

    #[test]
    fn boot_starts_the_greeter_instead_of_the_user_shell() {
        let compositor: DaemonDefinition =
            toml::from_str(include_str!("../daemons/sol-compositor.daemon")).unwrap();
        let logind: DaemonDefinition =
            toml::from_str(include_str!("../daemons/sol-logind.daemon")).unwrap();
        let shell: DaemonDefinition =
            toml::from_str(include_str!("../daemons/sol-shell.daemon")).unwrap();

        assert_eq!(compositor.daemon.start_mode, StartMode::Boot);
        assert_eq!(logind.daemon.start_mode, StartMode::Boot);
        assert_ne!(shell.daemon.start_mode, StartMode::Boot);
        assert!(logind
            .daemon
            .requires
            .iter()
            .any(|name| name == "sol-compositor"));
    }
}
