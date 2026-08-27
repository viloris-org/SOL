#!/bin/bash
# Create the bootable SOL ISO image.
#
# The ISO is UEFI-only and boots SOL's own bootloader (sol-boot) instead of
# GRUB. Layout:
#
#   /live/filesystem.squashfs          - the SOL root filesystem (squashfs)
#   /boot/esp.img                      - El Torito EFI boot image ("the ESP")
#                                        EFI/BOOT/BOOTX64.EFI        sol-boot
#                                        EFI/SOL/sol-boot.efi        sol-boot
#                                        EFI/SOL/slots/A/system.efi  UKI (kernel)
#                                        EFI/SOL/slots/A/initrd.img  SOL initramfs
#                                        EFI/SOL/slots/A/manifest.json
#                                        EFI/SOL/slots/A/deployment.bin
#                                        EFI/SOL/state/{state-a.bin,state-b.bin}
#                                        EFI/SOL/recovery/recovery-{a,b}.efi
#
# sol-boot verifies the signed slot and transfers control to the UKI through
# EFI LoadImage/StartImage. The kernel (CONFIG_EFI_STUB) reads its command
# line from CONFIG_CMDLINE and loads EFI/SOL/slots/A/initrd.img from its own
# volume; the initramfs then mounts the squashfs from the CD and hands control
# to /sbin/init -> sol-init.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILD_DIR="${PROJECT_ROOT}/build"
ROOTFS_DIR="${BUILD_DIR}/rootfs-staging"
ISO_DIR="${BUILD_DIR}/iso-staging"
ISO_OUTPUT="${BUILD_DIR}/iso"
ESP_DIR="${BUILD_DIR}/esp"
ESP_IMG="${BUILD_DIR}/esp.img"

# Get version and codename
SOL_VERSION=$(cat "${PROJECT_ROOT}/VERSION" 2>/dev/null || \
    git -C "$PROJECT_ROOT" describe --tags --always 2>/dev/null || echo "dev")
SOL_CODENAME=$(cat "${PROJECT_ROOT}/CODENAME" 2>/dev/null || echo "")
SOL_FULL_VERSION="${SOL_VERSION}${SOL_CODENAME:+ (${SOL_CODENAME})}"
KERNEL_VERSION=$(cat "${BUILD_DIR}/kernel-version.txt" 2>/dev/null || echo "unknown")
ISO_NAME="sol-${SOL_VERSION}-x86_64.iso"

SOL_IMAGE_BIN="${PROJECT_ROOT}/target/release/sol-image"

echo "==> Creating SOL ISO image (sol-boot UEFI)..."
echo "  Version: ${SOL_FULL_VERSION}"
echo "  Kernel: ${KERNEL_VERSION}"

if [ ! -x "${SOL_IMAGE_BIN}" ]; then
    echo "ERROR: sol-image binary not found. Run scripts/iso/build-platform.sh first."
    exit 1
fi
if [ ! -f "${BUILD_DIR}/kernel-artifacts/vmlinuz" ]; then
    echo "ERROR: kernel image not found. Run scripts/iso/build-kernel.sh first."
    exit 1
fi
if [ ! -d "${ROOTFS_DIR}" ]; then
    echo "ERROR: rootfs staging not found. Run scripts/iso/assemble-rootfs.sh first."
    exit 1
fi

rm -rf "${ISO_DIR}" "${ESP_DIR}"
mkdir -p "${ISO_DIR}/live" "${ISO_DIR}/boot"
mkdir -p "${ESP_DIR}/EFI/BOOT" "${ESP_DIR}/EFI/SOL/slots/A" \
    "${ESP_DIR}/EFI/SOL/state" "${ESP_DIR}/EFI/SOL/recovery"
mkdir -p "${ISO_OUTPUT}"

# ---------------------------------------------------------------------------
# 1. Root filesystem squashfs
# ---------------------------------------------------------------------------
echo "==> Creating squashfs filesystem..."
if ! command -v mksquashfs &> /dev/null; then
    echo "ERROR: mksquashfs not found. Install squashfs-tools."
    exit 1
fi
sudo mksquashfs "${ROOTFS_DIR}" "${ISO_DIR}/live/filesystem.squashfs" \
    -comp xz \
    -Xbcj x86 \
    -b 1M \
    -noappend \
    -e boot
echo "  ✓ Squashfs: $(du -h "${ISO_DIR}/live/filesystem.squashfs" | cut -f1)"

# ---------------------------------------------------------------------------
# 2. Initramfs (busybox-based, SOL-owned)
# ---------------------------------------------------------------------------
"${SCRIPT_DIR}/build-initramfs.sh"
INITRAMFS="${BUILD_DIR}/initramfs/initramfs.cpio.gz"
VMLINUZ="${BUILD_DIR}/kernel-artifacts/vmlinuz"

# ---------------------------------------------------------------------------
# 3. UKI - the kernel itself (CONFIG_EFI_STUB makes bzImage a PE32+ EFI app)
# ---------------------------------------------------------------------------
echo "==> Preparing UKI (kernel EFI stub image)..."
cp "${VMLINUZ}" "${ESP_DIR}/EFI/SOL/slots/A/system.efi"
cp "${INITRAMFS}" "${ESP_DIR}/EFI/SOL/slots/A/initrd.img"

# ---------------------------------------------------------------------------
# 4. Build sol-boot (UEFI) with a development release key
# ---------------------------------------------------------------------------
echo "==> Building sol-boot UEFI application..."
RELEASE_KEY="${BUILD_DIR}/esp-keys/release.key"
mkdir -p "$(dirname "${RELEASE_KEY}")"
dd if=/dev/zero of="${RELEASE_KEY}" bs=32 count=1 status=none
RELEASE_PUBLIC_KEY=$("${SOL_IMAGE_BIN}" release-public-key --signing-key "${RELEASE_KEY}")

rustup target add x86_64-unknown-uefi >/dev/null 2>&1 || true
SOL_BOOT_PUBLIC_KEY_HEX="${RELEASE_PUBLIC_KEY}" cargo build \
    --manifest-path "${PROJECT_ROOT}/Cargo.toml" \
    -p sol-boot \
    --bin sol-boot \
    --bin sol-boot-test-payload \
    --features ovmf-test-payload \
    --target x86_64-unknown-uefi \
    --release

UEFI_TARGET_DIR="${PROJECT_ROOT}/target/x86_64-unknown-uefi/release"
SOL_BOOT_EFI="${UEFI_TARGET_DIR}/sol-boot.efi"
SOL_BOOT_PAYLOAD="${UEFI_TARGET_DIR}/sol-boot-test-payload.efi"
if [ ! -f "${SOL_BOOT_EFI}" ]; then
    echo "ERROR: sol-boot.efi not produced (target x86_64-unknown-uefi missing?)"
    exit 1
fi

cp "${SOL_BOOT_EFI}" "${ESP_DIR}/EFI/BOOT/BOOTX64.EFI"
cp "${SOL_BOOT_EFI}" "${ESP_DIR}/EFI/SOL/sol-boot.efi"
# Independent recovery image (diagnostic payload) so sol-boot never fails
# open into firmware setup on this development image.
cp "${SOL_BOOT_PAYLOAD}" "${ESP_DIR}/EFI/SOL/recovery/recovery-a.efi"
cp "${SOL_BOOT_PAYLOAD}" "${ESP_DIR}/EFI/SOL/recovery/recovery-b.efi"

# ---------------------------------------------------------------------------
# 5. Provision the signed slot A deployment
# ---------------------------------------------------------------------------
echo "==> Provisioning signed deployment slot A..."
SQUASHFS_HASH=$(sha256sum "${ISO_DIR}/live/filesystem.squashfs" | cut -d ' ' -f 1)

"${SOL_IMAGE_BIN}" manifest \
    --slot A \
    --generation 1 \
    --version "${SOL_VERSION}-${SOL_CODENAME}" \
    --kernel "${VMLINUZ}" \
    --initrd "${INITRAMFS}" \
    --root-image "${ISO_DIR}/live/filesystem.squashfs" \
    --uki "${ESP_DIR}/EFI/SOL/slots/A/system.efi" \
    --kernel-component "kernel-x86_64:${SOL_VERSION}" \
    --initrd-component "initrd-base:${SOL_VERSION}" \
    --dm-verity-root-hash "${SQUASHFS_HASH}" \
    --dm-verity-slot-root "slot-a-${SOL_VERSION}" \
    --runtime "sol-runtime-1:1:dev" \
    --output "${ESP_DIR}/EFI/SOL/slots/A/manifest.json"

"${SOL_IMAGE_BIN}" boot-descriptor \
    --slot A \
    --generation 1 \
    --manifest "${ESP_DIR}/EFI/SOL/slots/A/manifest.json" \
    --uki "${ESP_DIR}/EFI/SOL/slots/A/system.efi" \
    --signing-key "${RELEASE_KEY}" \
    --output "${ESP_DIR}/EFI/SOL/slots/A/deployment.bin"

"${SOL_IMAGE_BIN}" init-boot-state \
    --slot A \
    --generation 1 \
    --state-a "${ESP_DIR}/EFI/SOL/state/state-a.bin" \
    --state-b "${ESP_DIR}/EFI/SOL/state/state-b.bin"

echo "  ✓ Slot A signed and state initialized"

# ---------------------------------------------------------------------------
# 6. ESP FAT image for the El Torito boot entry
# ---------------------------------------------------------------------------
echo "==> Creating EFI system partition image..."
if ! command -v mkfs.vfat &> /dev/null || ! command -v mcopy &> /dev/null; then
    echo "ERROR: mkfs.vfat (dosfstools) and mcopy (mtools) are required."
    exit 1
fi
rm -f "${ESP_IMG}"
mkfs.vfat -F 32 -n SOL_ESP -C "${ESP_IMG}" 65536 > /dev/null
mcopy -s -i "${ESP_IMG}" "${ESP_DIR}"/* ::/
cp "${ESP_IMG}" "${ISO_DIR}/boot/esp.img"
echo "  ✓ ESP image: $(du -h "${ESP_IMG}" | cut -f1)"

# ---------------------------------------------------------------------------
# 7. Generate the ISO (UEFI-only, via xorriso; no GRUB anywhere)
# ---------------------------------------------------------------------------
echo "==> Generating ISO image (xorriso)..."
if ! command -v xorriso &> /dev/null; then
    echo "ERROR: xorriso not found. Install xorriso."
    exit 1
fi
xorriso -as mkisofs \
    -o "${ISO_OUTPUT}/${ISO_NAME}" \
    -volid "SOL_OS" \
    -J -joliet-long \
    -rational-rock \
    -eltorito-alt-boot \
    -e boot/esp.img \
    -no-emul-boot \
    -isohybrid-gpt-basdat \
    "${ISO_DIR}"

# Generate checksums
echo "==> Generating checksums..."
cd "${ISO_OUTPUT}"
sha256sum "${ISO_NAME}" > SHA256SUMS
md5sum "${ISO_NAME}" > MD5SUMS

ISO_SIZE=$(du -h "${ISO_OUTPUT}/${ISO_NAME}" | cut -f1)
echo ""
echo "✓ ISO image created successfully!"
echo "  File: ${ISO_OUTPUT}/${ISO_NAME}"
echo "  Size: ${ISO_SIZE}"
echo "  SHA256: $(cut -d' ' -f1 SHA256SUMS)"
echo ""
echo "To test the ISO:"
echo "  qemu-system-x86_64 -m 2G -smp 2 \\"
echo "    -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE.fd \\"
echo "    -drive if=pflash,format=raw,file=/usr/share/OVMF/OVMF_VARS.fd \\"
echo "    -cdrom ${ISO_OUTPUT}/${ISO_NAME}"
echo ""
echo "To write to USB (UEFI):"
echo "  sudo dd if=${ISO_OUTPUT}/${ISO_NAME} of=/dev/sdX bs=4M status=progress"