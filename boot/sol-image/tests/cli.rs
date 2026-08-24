#![allow(clippy::expect_used)]

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
