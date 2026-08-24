#!/usr/bin/env bash
set -euo pipefail

: "${SOL_BOOT_PUBLIC_KEY_HEX:?set SOL_BOOT_PUBLIC_KEY_HEX to the 64-digit Ed25519 release public key}"

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
rustup target add x86_64-unknown-uefi
cargo build \
  --manifest-path "$repo_root/Cargo.toml" \
  -p sol-boot \
  --bin sol-boot \
  --target x86_64-unknown-uefi \
  --release

efi="$repo_root/target/x86_64-unknown-uefi/release/sol-boot.efi"
file "$efi"
printf '%s\n' "$efi"
