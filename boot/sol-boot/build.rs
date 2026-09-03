//! Emits the release verification key compiled into the UEFI policy adapter.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=SOL_BOOT_PUBLIC_KEY_HEX");
    println!("cargo:rerun-if-env-changed=SOL_BOOT_TEST_PAYLOAD_MARKER");
    if let Err(error) = emit_release_key() {
        panic!("{error}");
    }
    let marker = env::var("SOL_BOOT_TEST_PAYLOAD_MARKER").unwrap_or_else(|_| "GENERIC".to_owned());
    println!("cargo:rustc-env=SOL_BOOT_TEST_PAYLOAD_MARKER={marker}");
}

fn emit_release_key() -> Result<(), String> {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("uefi") {
        return Ok(());
    }
    let encoded = env::var("SOL_BOOT_PUBLIC_KEY_HEX").map_err(|_| {
        "SOL_BOOT_PUBLIC_KEY_HEX must contain the 64-hex-digit Ed25519 release public key"
            .to_owned()
    })?;
    if encoded.len() != 64 || !encoded.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(
            "SOL_BOOT_PUBLIC_KEY_HEX must contain exactly 64 hexadecimal digits".to_owned(),
        );
    }
    let mut values = Vec::with_capacity(32);
    for offset in (0..64).step_by(2) {
        values.push(
            u8::from_str_radix(&encoded[offset..offset + 2], 16)
                .map_err(|error| format!("invalid public key: {error}"))?,
        );
    }
    let out_dir = env::var_os("OUT_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| "Cargo did not provide OUT_DIR".to_owned())?;
    fs::write(
        out_dir.join("release_key.rs"),
        format!("pub const RELEASE_KEY: [u8; 32] = {values:?};\n"),
    )
    .map_err(|error| format!("cannot emit release key: {error}"))
}
