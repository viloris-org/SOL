#![allow(clippy::expect_used)]

use std::fs;
use std::path::Path;

use prost::Message as _;
use sol_bundle::proto::SolSignatureV2;
use sol_bundle::{
    BundleError, CacheState, GrantInheritance, PrivateKey, RevocationCache, RevocationEntry,
    RotationMetadata, SignatureAlgorithm, add_signer, check_grant_inheritance, check_update,
    fingerprint, generate_key, read_lineage, rotate_lineage, sign_bundle, verify_app_bundle,
    verify_lineage, write_lineage,
};
use tempfile::TempDir;

fn create_bundle(root: &Path, name: &str, version_code: u64) -> std::path::PathBuf {
    let bundle = root.join(name);
    fs::create_dir_all(bundle.join("bin/x86_64-linux")).expect("create bin");
    fs::create_dir_all(bundle.join("resources")).expect("create resources");
    fs::create_dir_all(bundle.join("metadata")).expect("create metadata");
    fs::write(
        bundle.join("App.toml"),
        format!(
            "[app]\napp_id = \"com.example.editor\"\nversion = \"1.0.{version_code}\"\nversion_code = {version_code}\n"
        ),
    )
    .expect("write App.toml");
    fs::write(bundle.join("bin/x86_64-linux/editor"), b"program").expect("write executable");
    fs::write(bundle.join("resources/icon.txt"), b"icon").expect("write resource");
    fs::write(bundle.join("metadata/sbom.json"), b"{}\n").expect("write metadata");
    bundle
}

fn generate(root: &Path, name: &str, algorithm: SignatureAlgorithm) -> PrivateKey {
    let path = root.join(name);
    generate_key(&path, algorithm).expect("generate key");
    PrivateKey::read(&path, algorithm).expect("read key")
}

#[test]
fn every_supported_algorithm_round_trips() {
    let temporary = TempDir::new().expect("temporary directory");
    for (index, algorithm) in [
        SignatureAlgorithm::Ed25519,
        SignatureAlgorithm::EcdsaP256Sha256,
        SignatureAlgorithm::Rsa4096Sha256,
    ]
    .into_iter()
    .enumerate()
    {
        let bundle = create_bundle(temporary.path(), &format!("Editor{index}.app"), 1);
        let key = generate(temporary.path(), &format!("key-{index}.pem"), algorithm);
        let signed = sign_bundle(&bundle, &key, None, 7, 1_777_777_777).expect("sign");
        let verified = verify_app_bundle(&bundle, None).expect("verify");
        assert_eq!(signed, verified);
        assert_eq!(verified.min_sol_version, 7);
        assert_eq!(verified.all_signers[0].algorithm, algorithm);
        assert_eq!(verified.publisher_lineage.chain.len(), 1);
    }
}

#[test]
fn detects_changed_and_added_content() {
    let temporary = TempDir::new().expect("temporary directory");
    let bundle = create_bundle(temporary.path(), "Editor.app", 1);
    let key = generate(
        temporary.path(),
        "publisher.pem",
        SignatureAlgorithm::Ed25519,
    );
    sign_bundle(&bundle, &key, None, 1, 100).expect("sign");

    fs::write(bundle.join("resources/icon.txt"), b"tampered").expect("tamper resource");
    assert!(matches!(
        verify_app_bundle(&bundle, None),
        Err(BundleError::DigestMismatch(path)) if path == "resources/icon.txt"
    ));

    fs::write(bundle.join("resources/icon.txt"), b"icon").expect("restore resource");
    fs::write(bundle.join("injected.dat"), b"malware").expect("inject file");
    assert!(matches!(
        verify_app_bundle(&bundle, None),
        Err(BundleError::DigestMismatch(path)) if path == "injected.dat"
    ));
}

#[test]
fn rotation_preserves_identity_and_downgrade_is_rejected() {
    let temporary = TempDir::new().expect("temporary directory");
    let key_a = generate(temporary.path(), "a.pem", SignatureAlgorithm::Ed25519);
    let key_b = generate(temporary.path(), "b.pem", SignatureAlgorithm::Ed25519);
    let old_bundle = create_bundle(temporary.path(), "Old.app", 1);
    let new_bundle = create_bundle(temporary.path(), "New.app", 2);
    let old = sign_bundle(&old_bundle, &key_a, None, 1, 100).expect("sign old");

    let lineage = rotate_lineage(
        None,
        &key_a,
        &key_b,
        RotationMetadata {
            reason: "key_expiry".to_owned(),
            timestamp: 200,
            description: "planned rotation".to_owned(),
        },
    )
    .expect("rotate");
    let lineage_path = temporary.path().join("lineage.bin");
    write_lineage(&lineage_path, &lineage).expect("write lineage");
    let decoded = read_lineage(&lineage_path).expect("read lineage");
    assert_eq!(lineage, decoded);
    let new = sign_bundle(&new_bundle, &key_b, Some(&lineage), 1, 300).expect("sign new");
    assert!(matches!(
        check_update(&old, &new),
        Ok(GrantInheritance::SameLineage { .. })
    ));
    assert!(matches!(
        check_update(&new, &old),
        Err(BundleError::DowngradeAttempt { .. })
    ));
}

#[test]
fn discontinuous_publisher_does_not_inherit() {
    let temporary = TempDir::new().expect("temporary directory");
    let old_bundle = create_bundle(temporary.path(), "Old.app", 1);
    let new_bundle = create_bundle(temporary.path(), "New.app", 2);
    let old_key = generate(temporary.path(), "old.pem", SignatureAlgorithm::Ed25519);
    let unrelated = generate(
        temporary.path(),
        "unrelated.pem",
        SignatureAlgorithm::Ed25519,
    );
    let old = sign_bundle(&old_bundle, &old_key, None, 1, 100).expect("sign old");
    let new = sign_bundle(&new_bundle, &unrelated, None, 1, 200).expect("sign new");
    assert_eq!(
        check_grant_inheritance(&old, &new),
        GrantInheritance::Discontinuous
    );
}

#[test]
fn every_signer_must_remain_valid() {
    let temporary = TempDir::new().expect("temporary directory");
    let bundle = create_bundle(temporary.path(), "Editor.app", 1);
    let key_a = generate(temporary.path(), "a.pem", SignatureAlgorithm::Ed25519);
    let key_b = generate(
        temporary.path(),
        "b.pem",
        SignatureAlgorithm::EcdsaP256Sha256,
    );
    sign_bundle(&bundle, &key_a, None, 1, 100).expect("sign first");
    let identity = add_signer(&bundle, &key_b, None, 200).expect("add signer");
    assert_eq!(identity.all_signers.len(), 2);

    let path = bundle.join(".signatures/v2.sig");
    let mut block =
        SolSignatureV2::decode(fs::read(&path).expect("read v2").as_slice()).expect("decode v2");
    block.signers[1].signatures[0].value[0] ^= 0x80;
    fs::write(&path, block.encode_to_vec()).expect("corrupt second signer");
    assert!(matches!(
        verify_app_bundle(&bundle, None),
        Err(BundleError::InvalidSignature { signer: 1, .. })
    ));
}

#[test]
fn rejects_duplicate_lineage_key() {
    let temporary = TempDir::new().expect("temporary directory");
    let key_a = generate(temporary.path(), "a.pem", SignatureAlgorithm::Ed25519);
    let key_b = generate(temporary.path(), "b.pem", SignatureAlgorithm::Ed25519);
    let mut lineage = rotate_lineage(
        None,
        &key_a,
        &key_b,
        RotationMetadata {
            reason: "test".to_owned(),
            timestamp: 1,
            description: String::new(),
        },
    )
    .expect("rotate");
    lineage.signers[1].certificate = lineage.signers[0].certificate.clone();
    assert!(matches!(
        verify_lineage(&lineage),
        Err(BundleError::InvalidLineage(_))
    ));
}

#[test]
fn revocation_cutoff_and_cache_state_are_enforced() {
    let temporary = TempDir::new().expect("temporary directory");
    let bundle = create_bundle(temporary.path(), "Editor.app", 1);
    let key = generate(temporary.path(), "key.pem", SignatureAlgorithm::Ed25519);
    let identity = sign_bundle(&bundle, &key, None, 1, 200).expect("sign");
    let cache = RevocationCache {
        last_sync: 1_000,
        sync_interval_hours: 24,
        entries: vec![RevocationEntry {
            key_fingerprint: fingerprint(&identity.publisher_lineage.current_key),
            revoked_after: 200,
            reason: "key_compromise".to_owned(),
            safe_replacement: None,
        }],
    };
    assert_eq!(cache.state_at(1_000 + 23 * 3_600), CacheState::Fresh);
    assert_eq!(cache.state_at(1_000 + 24 * 3_600), CacheState::Stale);
    assert_eq!(cache.state_at(1_000 + 48 * 3_600), CacheState::Expired);
    assert!(matches!(
        verify_app_bundle(&bundle, Some(&cache)),
        Err(BundleError::KeyRevoked { .. })
    ));
}

#[cfg(unix)]
#[test]
fn symbolic_links_are_rejected() {
    use std::os::unix::fs::symlink;

    let temporary = TempDir::new().expect("temporary directory");
    let bundle = create_bundle(temporary.path(), "Editor.app", 1);
    symlink("icon.txt", bundle.join("resources/alias.txt")).expect("create symlink");
    let key = generate(temporary.path(), "key.pem", SignatureAlgorithm::Ed25519);
    assert!(matches!(
        sign_bundle(&bundle, &key, None, 1, 100),
        Err(BundleError::InvalidLayout(message)) if message.contains("symbolic links")
    ));
}
