#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
efi="$repo_root/target/x86_64-unknown-uefi/release/sol-boot.efi"
ovmf_code=${OVMF_CODE:-/usr/share/edk2/x64/OVMF_CODE.4m.fd}
ovmf_vars=${OVMF_VARS:-/usr/share/edk2/x64/OVMF_VARS.4m.fd}

test_root=$(mktemp -d /tmp/sol-boot-ovmf.XXXXXX)
cleanup() {
  rm -rf -- "$test_root"
}
trap cleanup EXIT

mkdir -p "$test_root/esp/EFI/BOOT"
cp "$efi" "$test_root/esp/EFI/BOOT/BOOTX64.EFI"
cp "$ovmf_vars" "$test_root/OVMF_VARS.fd"

set +e
timeout 15s qemu-system-x86_64 \
  -machine q35,accel=tcg \
  -m 256M \
  -nodefaults \
  -no-reboot \
  -nographic \
  -serial stdio \
  -monitor none \
  -drive if=pflash,format=raw,readonly=on,file="$ovmf_code" \
  -drive if=pflash,format=raw,file="$test_root/OVMF_VARS.fd" \
  -drive format=raw,file=fat:rw:"$test_root/esp" \
  2>&1 | tee "$test_root/serial.log"
qemu_status=${PIPESTATUS[0]}
set -e

grep -q "SOL boot 0.1" "$test_root/serial.log"
grep -q "boot policy failed closed" "$test_root/serial.log"
if [[ $qemu_status -ne 0 && $qemu_status -ne 124 ]]; then
  exit "$qemu_status"
fi
printf '%s\n' "OVMF executed sol-boot and observed the expected fail-closed recovery path"
