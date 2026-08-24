#![allow(clippy::expect_used)]

use ed25519_dalek::{SigningKey, Verifier};
use sol_boot_core::{
    BootSuccessReport, DeploymentSlot, DeploymentStatus, DurableBootState,
    SignedDeploymentDescriptor, select_redundant_state,
};
use std::fs;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn cli_builds_and_verifies_a_complete_deployment() {
    let directory = tempdir().expect("fixture directory");
    let kernel = directory.path().join("vmlinuz");
    let initrd = directory.path().join("initrd.img");
    let root = directory.path().join("root.img");
    let manifest = directory.path().join("deployments/B/manifest.json");
    fs::write(&kernel, b"kernel").expect("kernel fixture");
    fs::write(&initrd, b"initrd").expect("initrd fixture");
    fs::write(&root, b"root image").expect("root fixture");

    let create = Command::new(env!("CARGO_BIN_EXE_sol-image"))
        .args([
            "manifest",
            "--slot",
            "B",
            "--generation",
            "42",
            "--version",
            "0.2.0-dev",
            "--kernel",
        ])
        .arg(&kernel)
        .arg("--initrd")
        .arg(&initrd)
        .arg("--root-image")
        .arg(&root)
        .args([
            "--runtime",
            "sol-runtime-1:12:documents.v2,accessibility.tree-v1",
            "--output",
        ])
        .arg(&manifest)
        .output()
        .expect("run manifest command");
    assert!(
        create.status.success(),
        "{}",
        String::from_utf8_lossy(&create.stderr)
    );

    let verify = || {
        Command::new(env!("CARGO_BIN_EXE_sol-image"))
            .arg("verify")
            .arg("--manifest")
            .arg(&manifest)
            .arg("--kernel")
            .arg(&kernel)
            .arg("--initrd")
            .arg(&initrd)
            .arg("--root-image")
            .arg(&root)
            .output()
            .expect("run verify command")
    };
    let valid = verify();
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );

    let unexpected_uki = directory.path().join("unexpected.efi");
    fs::write(&unexpected_uki, b"not bound by format 1").expect("unexpected UKI");
    let invalid = Command::new(env!("CARGO_BIN_EXE_sol-image"))
        .arg("verify")
        .arg("--manifest")
        .arg(&manifest)
        .arg("--kernel")
        .arg(&kernel)
        .arg("--initrd")
        .arg(&initrd)
        .arg("--root-image")
        .arg(&root)
        .arg("--uki")
        .arg(&unexpected_uki)
        .output()
        .expect("run format 1 verification with UKI");
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("not valid for a format 1"));

    fs::write(&root, b"tampered root image").expect("tamper root fixture");
    let tampered = verify();
    assert!(!tampered.status.success());
    assert!(String::from_utf8_lossy(&tampered.stderr).contains("root image does not match"));
}

#[test]
fn cli_builds_and_verifies_a_v2_uki_aware_deployment() {
    let directory = tempdir().expect("fixture directory");
    let kernel = directory.path().join("vmlinuz");
    let initrd = directory.path().join("initrd.img");
    let root = directory.path().join("root.img");
    let uki = directory.path().join("uki.efi");
    let manifest = directory.path().join("deployments/B/manifest-v2.json");
    fs::write(&kernel, b"kernel-v2").expect("kernel fixture");
    fs::write(&initrd, b"initrd-v2").expect("initrd fixture");
    fs::write(&root, b"root-v2").expect("root fixture");
    fs::write(&uki, b"uki-contents-v2").expect("uki fixture");

    let create = Command::new(env!("CARGO_BIN_EXE_sol-image"))
        .args([
            "manifest",
            "--slot",
            "B",
            "--generation",
            "42",
            "--version",
            "0.3.0-dev",
            "--kernel",
        ])
        .arg(&kernel)
        .arg("--initrd")
        .arg(&initrd)
        .arg("--root-image")
        .arg(&root)
        .arg("--uki")
        .arg(&uki)
        .args([
            "--kernel-component",
            "kernel-x86_64:slot-b-gen-42-kernel-abc123",
            "--initrd-component",
            "initrd-base:slot-b-gen-42-initrd-def456",
            "--dm-verity-root-hash",
            &"a".repeat(64),
            "--dm-verity-slot-root",
            "slot-b-root-abc123",
            "--runtime",
            "sol-runtime-1:12:documents.v2",
            "--output",
        ])
        .arg(&manifest)
        .output()
        .expect("run V2 manifest command");
    assert!(
        create.status.success(),
        "V2 manifest creation failed: {}",
        String::from_utf8_lossy(&create.stderr)
    );

    let verify = || {
        Command::new(env!("CARGO_BIN_EXE_sol-image"))
            .arg("verify")
            .arg("--manifest")
            .arg(&manifest)
            .arg("--kernel")
            .arg(&kernel)
            .arg("--initrd")
            .arg(&initrd)
            .arg("--root-image")
            .arg(&root)
            .arg("--uki")
            .arg(&uki)
            .output()
            .expect("run verify command")
    };
    let missing_uki = Command::new(env!("CARGO_BIN_EXE_sol-image"))
        .arg("verify")
        .arg("--manifest")
        .arg(&manifest)
        .arg("--kernel")
        .arg(&kernel)
        .arg("--initrd")
        .arg(&initrd)
        .arg("--root-image")
        .arg(&root)
        .output()
        .expect("run format 2 verification without UKI");
    assert!(!missing_uki.status.success());
    assert!(String::from_utf8_lossy(&missing_uki.stderr).contains("--uki is required"));

    let valid = verify();
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );

    fs::write(&root, b"tampered root").expect("tamper root");
    let tampered = verify();
    assert!(!tampered.status.success());
    assert!(String::from_utf8_lossy(&tampered.stderr).contains("root image does not match"));

    fs::write(&root, b"root-v2").expect("restore root");
    fs::write(&uki, b"tampered UKI").expect("tamper UKI");
    let tampered = verify();
    assert!(!tampered.status.success());
    assert!(String::from_utf8_lossy(&tampered.stderr).contains("UKI does not match"));
}

#[test]
fn cli_rejects_incomplete_v2_flags() {
    let directory = tempdir().expect("fixture directory");
    let kernel = directory.path().join("vmlinuz");
    let initrd = directory.path().join("initrd.img");
    let root = directory.path().join("root.img");
    let uki = directory.path().join("uki.efi");
    let output = directory.path().join("manifest.json");
    fs::write(&kernel, b"kernel-v2").expect("kernel");
    fs::write(&initrd, b"initrd-v2").expect("initrd");
    fs::write(&root, b"root-v2").expect("root");
    fs::write(&uki, b"uki-v2").expect("uki");

    // Provide --uki but omit --kernel-component — must fail.
    let incomplete = Command::new(env!("CARGO_BIN_EXE_sol-image"))
        .args([
            "manifest",
            "--slot",
            "B",
            "--generation",
            "1",
            "--version",
            "0.1.0",
            "--kernel",
        ])
        .arg(&kernel)
        .arg("--initrd")
        .arg(&initrd)
        .arg("--root-image")
        .arg(&root)
        .arg("--uki")
        .arg(&uki)
        .args(["--runtime", "sol-runtime-1:1", "--output"])
        .arg(&output)
        .output()
        .expect("run incomplete V2 command");
    assert!(
        !incomplete.status.success(),
        "incomplete V2 flags should fail: {}",
        String::from_utf8_lossy(&incomplete.stderr)
    );
    assert!(String::from_utf8_lossy(&incomplete.stderr).contains("kernel-component is required"));
}

#[test]
fn cli_manages_release_key_and_redundant_boot_lifecycle_files() {
    let directory = tempdir().expect("fixture directory");
    let key = directory.path().join("release.key");
    let state_a = directory.path().join("state/state-a.bin");
    let state_b = directory.path().join("state/state-b.bin");
    let report = directory.path().join("state/success.bin");
    fs::write(&key, [7_u8; 32]).expect("release key");

    let public_key = Command::new(env!("CARGO_BIN_EXE_sol-image"))
        .args(["release-public-key", "--signing-key"])
        .arg(&key)
        .output()
        .expect("derive release public key");
    assert!(public_key.status.success());
    assert_eq!(String::from_utf8_lossy(&public_key.stdout).trim().len(), 64);

    let initialized = Command::new(env!("CARGO_BIN_EXE_sol-image"))
        .args([
            "init-boot-state",
            "--slot",
            "A",
            "--generation",
            "1",
            "--state-a",
        ])
        .arg(&state_a)
        .arg("--state-b")
        .arg(&state_b)
        .output()
        .expect("initialize state");
    assert!(
        initialized.status.success(),
        "{}",
        String::from_utf8_lossy(&initialized.stderr)
    );

    let staged = Command::new(env!("CARGO_BIN_EXE_sol-image"))
        .args([
            "stage-boot-trial",
            "--slot",
            "B",
            "--generation",
            "2",
            "--attempts",
            "3",
            "--state-a",
        ])
        .arg(&state_a)
        .arg("--state-b")
        .arg(&state_b)
        .output()
        .expect("stage state");
    assert!(
        staged.status.success(),
        "{}",
        String::from_utf8_lossy(&staged.stderr)
    );
    let a = fs::read(&state_a).expect("state A");
    let b = fs::read(&state_b).expect("state B");
    let selected = select_redundant_state(Some(&a), Some(&b)).expect("select staged state");
    assert_eq!(selected.envelope().sequence(), 2);
    assert!(matches!(
        selected
            .envelope()
            .state()
            .record(DeploymentSlot::B)
            .map(sol_boot_core::DeploymentRecord::status),
        Some(DeploymentStatus::Trial {
            remaining_attempts: 3,
            pending_attempt: None
        })
    ));
    assert_eq!(
        DurableBootState::from_canonical_bytes(&a).expect("decode A"),
        DurableBootState::from_canonical_bytes(&b).expect("decode B")
    );

    let reported = Command::new(env!("CARGO_BIN_EXE_sol-image"))
        .args([
            "success-report",
            "--slot",
            "B",
            "--generation",
            "2",
            "--attempt",
            "1",
            "--output",
        ])
        .arg(&report)
        .output()
        .expect("write success report");
    assert!(reported.status.success());
    let decoded =
        BootSuccessReport::from_canonical_bytes(&fs::read(report).expect("success report bytes"))
            .expect("decode report");
    assert_eq!(decoded.deployment.slot(), DeploymentSlot::B);
    assert_eq!(decoded.deployment.generation(), 2);
    assert_eq!(decoded.attempt.get(), 1);
}

#[test]
fn cli_signs_a_format_two_manifest_and_uki_binding() {
    let directory = tempdir().expect("fixture directory");
    let kernel = directory.path().join("vmlinuz");
    let initrd = directory.path().join("initrd.img");
    let root = directory.path().join("root.img");
    let uki = directory.path().join("system.efi");
    let manifest = directory.path().join("manifest.json");
    let key = directory.path().join("release.key");
    let descriptor = directory.path().join("deployment.bin");
    fs::write(&kernel, b"kernel").expect("kernel");
    fs::write(&initrd, b"initrd").expect("initrd");
    fs::write(&root, b"root").expect("root");
    fs::write(&uki, b"UKI PE bytes").expect("UKI");
    fs::write(&key, [9_u8; 32]).expect("key");

    let manifest_result = Command::new(env!("CARGO_BIN_EXE_sol-image"))
        .args([
            "manifest",
            "--slot",
            "A",
            "--generation",
            "7",
            "--version",
            "0.7.0",
            "--kernel",
        ])
        .arg(&kernel)
        .arg("--initrd")
        .arg(&initrd)
        .arg("--root-image")
        .arg(&root)
        .arg("--uki")
        .arg(&uki)
        .args([
            "--kernel-component",
            "kernel:gen-7-kernel",
            "--initrd-component",
            "initrd:gen-7-initrd",
            "--dm-verity-root-hash",
            &"b".repeat(64),
            "--dm-verity-slot-root",
            "slot-a-gen-7-root",
            "--runtime",
            "sol-runtime-1:1",
            "--output",
        ])
        .arg(&manifest)
        .output()
        .expect("create format 2 manifest");
    assert!(
        manifest_result.status.success(),
        "{}",
        String::from_utf8_lossy(&manifest_result.stderr)
    );

    let signed_result = Command::new(env!("CARGO_BIN_EXE_sol-image"))
        .args([
            "boot-descriptor",
            "--slot",
            "A",
            "--generation",
            "7",
            "--manifest",
        ])
        .arg(&manifest)
        .arg("--uki")
        .arg(&uki)
        .arg("--signing-key")
        .arg(&key)
        .arg("--output")
        .arg(&descriptor)
        .output()
        .expect("sign descriptor");
    assert!(
        signed_result.status.success(),
        "{}",
        String::from_utf8_lossy(&signed_result.stderr)
    );

    let signed = SignedDeploymentDescriptor::from_canonical_bytes(
        &fs::read(descriptor).expect("descriptor bytes"),
    )
    .expect("canonical signed descriptor");
    assert_eq!(signed.descriptor().deployment().slot(), DeploymentSlot::A);
    assert_eq!(signed.descriptor().deployment().generation(), 7);
    let public = SigningKey::from_bytes(&[9_u8; 32]).verifying_key();
    public
        .verify(
            &signed.descriptor().canonical_payload(),
            &ed25519_dalek::Signature::from_bytes(&signed.signature()),
        )
        .expect("valid release signature");
}
