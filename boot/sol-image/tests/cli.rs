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

    fs::write(&root, b"tampered root image").expect("tamper root fixture");
    let tampered = verify();
    assert!(!tampered.status.success());
    assert!(String::from_utf8_lossy(&tampered.stderr).contains("root image does not match"));
}
