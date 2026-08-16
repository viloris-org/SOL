//! DRM connector discovery and hotplug topology for the udev backend.
//!
//! The runtime probe reads the kernel's `/sys/class/drm` connector state. It
//! is paired with Smithay's `UdevBackend`: udev tells us *when* to rescan, and
//! sysfs is the source of the connected connector names and advertised modes.
//! Tests inject a fixture root; fixtures exercise parsing and reconciliation,
//! not hardware access.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::PathBuf,
};

use crate::outputs::OutputConfiguration;

/// The kernel's view of one connected DRM connector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorSnapshot {
    /// Kernel connector name, for example `card0-HDMI-A-1`.
    pub name: String,
    /// Modes advertised by the connector, in kernel preference order.
    pub modes: Vec<Mode>,
}

/// A display mode obtained from a DRM connector's sysfs `modes` file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Mode {
    /// Horizontal pixel count.
    pub width: i32,
    /// Vertical pixel count.
    pub height: i32,
}

impl Mode {
    fn parse(value: &str) -> Option<Self> {
        let (width, height) = value.trim().split_once('x')?;
        let width = width.parse().ok()?;
        let height = height.parse().ok()?;
        if width > 0 && height > 0 {
            Some(Self { width, height })
        } else {
            None
        }
    }
}

/// Reads DRM connector state from sysfs.
#[derive(Debug, Clone)]
pub struct SysfsDrmConnectorProbe {
    root: PathBuf,
}

impl Default for SysfsDrmConnectorProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl SysfsDrmConnectorProbe {
    /// Create a probe for the real kernel DRM sysfs root.
    #[must_use]
    pub fn new() -> Self {
        Self::at("/sys/class/drm")
    }

    /// Create a probe rooted at `root`.
    ///
    /// This is public so tests can use a filesystem fixture without claiming
    /// that it represents a DRM device.
    #[must_use]
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Return all currently connected connectors with at least one valid mode.
    pub fn connected(&self) -> io::Result<Vec<ConnectorSnapshot>> {
        let mut connectors = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();

            // `card0` is a DRM device node; connector entries have another
            // hyphen after the card number (for example card0-eDP-1).
            if !name.starts_with("card") || name.matches('-').count() < 2 {
                continue;
            }
            if fs::read_to_string(path.join("status"))?.trim() != "connected" {
                continue;
            }

            let modes = fs::read_to_string(path.join("modes"))?
                .lines()
                .filter_map(Mode::parse)
                .collect::<Vec<_>>();
            if !modes.is_empty() {
                connectors.push(ConnectorSnapshot { name, modes });
            }
        }
        connectors.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(connectors)
    }
}

/// A change produced while reconciling a udev-triggered connector rescan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopologyChange {
    /// A connector became usable with the listed output configuration.
    Added(OutputConfiguration),
    /// An existing connector's effective output configuration changed.
    Changed(OutputConfiguration),
    /// A connector was disconnected or stopped advertising usable modes.
    Removed(String),
}

/// Deterministic output placement and hotplug reconciliation.
///
/// Connected monitors are placed left-to-right using their first advertised
/// mode. Per-monitor user configuration can later replace this policy without
/// changing the udev or Wayland output ownership boundary.
#[derive(Debug, Default)]
pub struct OutputTopology {
    outputs: BTreeMap<String, OutputConfiguration>,
}

impl OutputTopology {
    /// Reconcile a fresh kernel connector snapshot and return the changes.
    pub fn reconcile(&mut self, connectors: Vec<ConnectorSnapshot>) -> Vec<TopologyChange> {
        let next = layout(connectors);
        let mut changes = Vec::new();

        for (name, configuration) in &next {
            match self.outputs.get(name) {
                None => changes.push(TopologyChange::Added(configuration.clone())),
                Some(current) if current != configuration => {
                    changes.push(TopologyChange::Changed(configuration.clone()));
                }
                Some(_) => {}
            }
        }
        for name in self.outputs.keys() {
            if !next.contains_key(name) {
                changes.push(TopologyChange::Removed(name.clone()));
            }
        }

        self.outputs = next;
        changes
    }

    /// Return the current configurations in deterministic layout order.
    #[must_use]
    pub fn configurations(&self) -> Vec<OutputConfiguration> {
        self.outputs.values().cloned().collect()
    }
}

fn layout(connectors: Vec<ConnectorSnapshot>) -> BTreeMap<String, OutputConfiguration> {
    let mut remaining = BTreeSet::new();
    let mut by_name = BTreeMap::new();
    for connector in connectors {
        if let Some(mode) = connector.modes.first().copied() {
            remaining.insert(connector.name.clone());
            by_name.insert(connector.name, mode);
        }
    }

    let mut x = 0;
    remaining
        .into_iter()
        .map(|name| {
            let mode = by_name[&name];
            let configuration =
                OutputConfiguration::new(name.clone(), (mode.width, mode.height), (x, 0));
            x += mode.width;
            (name, configuration)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{ConnectorSnapshot, Mode, OutputTopology, SysfsDrmConnectorProbe, TopologyChange};
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn connector(name: &str, modes: &[(i32, i32)]) -> ConnectorSnapshot {
        ConnectorSnapshot {
            name: name.into(),
            modes: modes
                .iter()
                .map(|&(width, height)| Mode { width, height })
                .collect(),
        }
    }

    fn fixture_root() -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("sol-drm-fixture-{unique}"));
        fs::create_dir_all(&root).expect("fixture root should be creatable");
        root
    }

    #[test]
    fn sysfs_probe_reads_only_connected_connectors_with_modes() {
        let root = fixture_root();
        let connected = root.join("card0-HDMI-A-1");
        let disconnected = root.join("card0-DP-1");
        fs::create_dir_all(&connected).expect("connected fixture should be creatable");
        fs::create_dir_all(&disconnected).expect("disconnected fixture should be creatable");
        fs::write(connected.join("status"), "connected\n").expect("status should write");
        fs::write(connected.join("modes"), "1920x1080\n1280x720\n").expect("modes should write");
        fs::write(disconnected.join("status"), "disconnected\n").expect("status should write");
        fs::write(disconnected.join("modes"), "2560x1440\n").expect("modes should write");

        let connectors = SysfsDrmConnectorProbe::at(&root)
            .connected()
            .expect("fixture should be readable");
        fs::remove_dir_all(&root).expect("fixture should be removable");

        assert_eq!(
            connectors,
            vec![connector("card0-HDMI-A-1", &[(1920, 1080), (1280, 720)])]
        );
    }

    #[test]
    fn topology_places_connected_outputs_left_to_right() {
        let mut topology = OutputTopology::default();
        let changes = topology.reconcile(vec![
            connector("card0-DP-1", &[(2560, 1440)]),
            connector("card0-HDMI-A-1", &[(1920, 1080)]),
        ]);

        assert_eq!(changes.len(), 2);
        assert_eq!(
            topology.configurations(),
            vec![
                crate::outputs::OutputConfiguration::new("card0-DP-1", (2560, 1440), (0, 0)),
                crate::outputs::OutputConfiguration::new("card0-HDMI-A-1", (1920, 1080), (2560, 0)),
            ]
        );
    }

    #[test]
    fn topology_reports_add_change_and_remove_for_hotplug() {
        let mut topology = OutputTopology::default();
        topology.reconcile(vec![connector("card0-eDP-1", &[(1920, 1200)])]);

        let changed = topology.reconcile(vec![connector("card0-eDP-1", &[(2560, 1600)])]);
        assert_eq!(
            changed,
            vec![TopologyChange::Changed(
                crate::outputs::OutputConfiguration::new("card0-eDP-1", (2560, 1600), (0, 0),)
            )]
        );

        assert_eq!(
            topology.reconcile(Vec::new()),
            vec![TopologyChange::Removed("card0-eDP-1".into())]
        );
    }
}
