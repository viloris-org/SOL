#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
ovmf_code=${OVMF_CODE:-/usr/share/edk2/x64/OVMF_CODE.4m.fd}
ovmf_vars=${OVMF_VARS:-/usr/share/edk2/x64/OVMF_VARS.4m.fd}

for dependency in cargo cmp cut dd qemu-system-x86_64 sha256sum timeout; do
  command -v "$dependency" >/dev/null || {
    printf '%s\n' "missing test dependency: $dependency" >&2
    exit 1
  }
done
for firmware in "$ovmf_code" "$ovmf_vars"; do
  [[ -f "$firmware" ]] || {
    printf '%s\n' "missing OVMF firmware: $firmware" >&2
    exit 1
  }
done

test_root=$(mktemp -d /tmp/sol-boot-ovmf.XXXXXX)
cleanup() {
  rm -rf -- "$test_root"
}
trap cleanup EXIT

target_dir="$test_root/target"
signing_key="$test_root/release.key"
dd if=/dev/zero of="$signing_key" bs=32 count=1 status=none
release_key=$(CARGO_TARGET_DIR="$target_dir" cargo run \
  --quiet --manifest-path "$repo_root/Cargo.toml" -p sol-image -- \
  release-public-key --signing-key "$signing_key")

SOL_BOOT_PUBLIC_KEY_HEX="$release_key" CARGO_TARGET_DIR="$target_dir" cargo build \
  --quiet --manifest-path "$repo_root/Cargo.toml" \
  -p sol-boot \
  --bin sol-boot \
  --bin sol-boot-test-payload \
  --features ovmf-test-payload \
  --target x86_64-unknown-uefi \
  --release

bootloader="$target_dir/x86_64-unknown-uefi/release/sol-boot.efi"
payload="$target_dir/x86_64-unknown-uefi/release/sol-boot-test-payload.efi"

run_ovmf() {
  local esp=$1
  local log=$2
  local vars=$3
  cp "$ovmf_vars" "$vars"
  set +e
  timeout 10s qemu-system-x86_64 \
    -machine q35,accel=tcg \
    -m 256M \
    -nodefaults \
    -no-reboot \
    -nographic \
    -serial stdio \
    -monitor none \
    -drive if=pflash,format=raw,readonly=on,file="$ovmf_code" \
    -drive if=pflash,format=raw,file="$vars" \
    -drive format=raw,file=fat:rw:"$esp" \
    >"$log" 2>&1
  local qemu_status=$?
  set -e
  if [[ $qemu_status -ne 0 && $qemu_status -ne 124 ]]; then
    cat "$log" >&2
    return "$qemu_status"
  fi
}

# A bare ESP must execute sol-boot and fail closed into recovery policy.
empty_esp="$test_root/empty-esp"
mkdir -p "$empty_esp/EFI/BOOT"
cp "$bootloader" "$empty_esp/EFI/BOOT/BOOTX64.EFI"
run_ovmf "$empty_esp" "$test_root/fail-closed.log" "$test_root/fail-closed-vars.fd"
grep -q "SOL boot 0.1" "$test_root/fail-closed.log"
grep -q "boot policy failed closed" "$test_root/fail-closed.log"
grep -q "no bootable image remained" "$test_root/fail-closed.log"

# Build two signed slots, stage B as a trial, and execute the selected child image.
esp="$test_root/trial-esp"
mkdir -p \
  "$esp/EFI/BOOT" \
  "$esp/EFI/SOL/slots/A" \
  "$esp/EFI/SOL/slots/B" \
  "$esp/EFI/SOL/state"
cp "$bootloader" "$esp/EFI/BOOT/BOOTX64.EFI"

printf '%s\n' 'test kernel' >"$test_root/kernel"
printf '%s\n' 'test initrd' >"$test_root/initrd"
printf '%s\n' 'test immutable root' >"$test_root/root.img"
root_hash=$(sha256sum "$test_root/root.img" | cut -d ' ' -f 1)

for slot_generation in A:1 B:2; do
  slot=${slot_generation%%:*}
  generation=${slot_generation##*:}
  slot_dir="$esp/EFI/SOL/slots/$slot"
  cp "$payload" "$slot_dir/system.efi"
  CARGO_TARGET_DIR="$target_dir" cargo run \
    --quiet --manifest-path "$repo_root/Cargo.toml" -p sol-image -- \
    manifest \
    --slot "$slot" \
    --generation "$generation" \
    --version "ovmf-$generation" \
    --kernel "$test_root/kernel" \
    --initrd "$test_root/initrd" \
    --root-image "$test_root/root.img" \
    --uki "$slot_dir/system.efi" \
    --kernel-component "kernel-x86_64:ovmf-$generation" \
    --initrd-component "initrd-base:ovmf-$generation" \
    --dm-verity-root-hash "$root_hash" \
    --dm-verity-slot-root "slot-${slot,,}-ovmf-$generation" \
    --runtime 'sol-runtime-1:1:ovmf-test' \
    --output "$slot_dir/manifest.json"
  CARGO_TARGET_DIR="$target_dir" cargo run \
    --quiet --manifest-path "$repo_root/Cargo.toml" -p sol-image -- \
    boot-descriptor \
    --slot "$slot" \
    --generation "$generation" \
    --manifest "$slot_dir/manifest.json" \
    --uki "$slot_dir/system.efi" \
    --signing-key "$signing_key" \
    --output "$slot_dir/deployment.bin"
done

CARGO_TARGET_DIR="$target_dir" cargo run \
  --quiet --manifest-path "$repo_root/Cargo.toml" -p sol-image -- \
  init-boot-state \
  --slot A \
  --generation 1 \
  --state-a "$esp/EFI/SOL/state/state-a.bin" \
  --state-b "$esp/EFI/SOL/state/state-b.bin"
CARGO_TARGET_DIR="$target_dir" cargo run \
  --quiet --manifest-path "$repo_root/Cargo.toml" -p sol-image -- \
  stage-boot-trial \
  --slot B \
  --generation 2 \
  --attempts 3 \
  --state-a "$esp/EFI/SOL/state/state-a.bin" \
  --state-b "$esp/EFI/SOL/state/state-b.bin"

run_ovmf "$esp" "$test_root/trial.log" "$test_root/trial-vars.fd"
grep -q "booting trial slot B generation 2 attempt 1" "$test_root/trial.log"
grep -q "SOL_BOOT_TEST_PAYLOAD_STARTED" "$test_root/trial.log"

CARGO_TARGET_DIR="$target_dir" cargo run \
  --quiet --manifest-path "$repo_root/Cargo.toml" -p sol-image -- \
  success-report \
  --slot B \
  --generation 2 \
  --attempt 1 \
  --output "$test_root/expected-success.bin"
cmp "$test_root/expected-success.bin" "$esp/EFI/SOL/state/current.bin"
cp "$esp/EFI/SOL/state/current.bin" "$esp/EFI/SOL/state/success.bin"

# The exact health report must promote B before the second transfer.
run_ovmf "$esp" "$test_root/promoted.log" "$test_root/promoted-vars.fd"
grep -q "booting known-good slot B generation 2" "$test_root/promoted.log"
grep -q "SOL_BOOT_TEST_PAYLOAD_STARTED" "$test_root/promoted.log"
# QEMU's vvfat directory backend does not reliably mirror guest-side deletion
# back to the host directory. The manager's host test covers report removal;
# reaching known-good B here proves that OVMF accepted and applied the report.

printf '%s\n' \
  "OVMF verified fail-closed recovery, durable trial boot, UKI transfer, and exact-report promotion"
