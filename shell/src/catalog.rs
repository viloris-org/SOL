//! Trusted application catalog entries bundled with SOL Shell.
//!
//! The package service will eventually supply the authenticated `.app`
//! catalog. Until that adapter exists, the Shell exposes only the first-party
//! applications compiled into the system image. Keeping this list explicit is
//! preferable to scanning executables: a filename is not an application
//! identity, and an arbitrary process must not be able to place itself in
//! trusted Shell UI by appearing on `PATH`.

use sol_app::{AppId, AppIdentity};

use crate::launcher::AppCatalogEntry;

/// The first-party applications present in the base SOL image.
///
/// These identities are compile-time platform data. Installed third-party
/// applications join this list only after the package-catalog adapter verifies
/// their bundle identity and publisher lineage.
#[must_use]
pub fn bundled_app_catalog() -> Vec<AppCatalogEntry> {
    vec![
        entry(
            "org.sol.files",
            "Files",
            &["documents", "folders", "file manager"],
        ),
        entry(
            "org.sol.settings",
            "Settings",
            &["preferences", "system", "configuration"],
        ),
        entry(
            "org.sol.terminal",
            "Terminal",
            &["shell", "console", "command line"],
        ),
    ]
}

fn entry(app_id: &str, display_name: &str, keywords: &[&str]) -> AppCatalogEntry {
    // These values are part of the system image and covered by tests below;
    // failure means the source itself violates SOL's identity contract.
    let app_id = AppId::parse(app_id).expect("bundled application ID must be valid");
    let identity = AppIdentity::new(app_id, display_name)
        .expect("bundled application display name must be valid");
    AppCatalogEntry::new(
        identity,
        keywords.iter().map(|keyword| (*keyword).to_owned()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_entries_are_unique_and_stably_ordered() {
        let catalog = bundled_app_catalog();
        let ids: Vec<_> = catalog
            .iter()
            .map(|entry| entry.app_id().as_str())
            .collect();

        assert_eq!(
            ids,
            ["org.sol.files", "org.sol.settings", "org.sol.terminal"]
        );
        assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn every_bundled_entry_is_searchable_by_a_local_keyword() {
        let catalog = bundled_app_catalog();
        assert!(catalog.iter().all(|entry| !entry.keywords.is_empty()));
    }
}
