use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        panic!("{error}");
    }
}

fn run() -> Result<(), String> {
    println!("cargo:rerun-if-env-changed=SOL_BOOT_PUBLIC_KEY_HEX");
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
    let source = format!("pub const RELEASE_KEY: [u8; 32] = {values:?};\n");
    let output = PathBuf::from(
        env::var_os("OUT_DIR").ok_or_else(|| "Cargo did not provide OUT_DIR".to_owned())?,
    )
    .join("release_key.rs");
    fs::write(output, source).map_err(|error| format!("cannot write release key: {error}"))?;
    Ok(())
}
