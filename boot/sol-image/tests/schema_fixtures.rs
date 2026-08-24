#![allow(clippy::expect_used)]

use sol_image::{DeploymentManifest, ManifestFormat};

#[test]
fn versioned_schema_fixtures_remain_canonical_and_distinct() {
    let v1_bytes = include_bytes!("fixtures/manifest-v1.json");
    let v2_bytes = include_bytes!("fixtures/manifest-v2.json");

    let v1 = DeploymentManifest::from_canonical_bytes(v1_bytes).expect("format 1 fixture");
    let v2 = DeploymentManifest::from_canonical_bytes(v2_bytes).expect("format 2 fixture");

    assert_eq!(v1.manifest_format(), Some(ManifestFormat::V1));
    assert!(v1.uki().is_none());
    assert_eq!(v1.canonical_bytes().expect("format 1 bytes"), v1_bytes);

    assert_eq!(v2.manifest_format(), Some(ManifestFormat::V2));
    assert!(v2.uki().is_some());
    assert_eq!(v2.canonical_bytes().expect("format 2 bytes"), v2_bytes);
    assert_ne!(v1_bytes.as_slice(), v2_bytes.as_slice());
}
