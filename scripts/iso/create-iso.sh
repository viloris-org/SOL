#!/bin/bash
# Create bootable SOL ISO image
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILD_DIR="${PROJECT_ROOT}/build"
ROOTFS_DIR="${BUILD_DIR}/rootfs-staging"
ISO_DIR="${BUILD_DIR}/iso-staging"
ISO_OUTPUT="${BUILD_DIR}/iso"

# Get version and codename
if [ -f "${PROJECT_ROOT}/VERSION" ]; then
    SOL_VERSION=$(cat "${PROJECT_ROOT}/VERSION")
else
    SOL_VERSION=$(git -C "$PROJECT_ROOT" describe --tags --always 2>/dev/null || echo "dev")
fi

if [ -f "${PROJECT_ROOT}/CODENAME" ]; then
    SOL_CODENAME=$(cat "${PROJECT_ROOT}/CODENAME")
    SOL_FULL_VERSION="${SOL_VERSION} (${SOL_CODENAME})"
else
    SOL_CODENAME=""
    SOL_FULL_VERSION="${SOL_VERSION}"
fi

KERNEL_VERSION=$(cat "${BUILD_DIR}/kernel-version.txt" 2>/dev/null || echo "unknown")
ISO_NAME="sol-${SOL_VERSION}-x86_64.iso"

echo "==> Creating SOL ISO image..."
echo "  Version: ${SOL_FULL_VERSION}"
echo "  Kernel: ${KERNEL_VERSION}"

# Clean and create ISO staging directory
rm -rf "${ISO_DIR}"
mkdir -p "${ISO_DIR}"/{boot/grub,live}
mkdir -p "${ISO_OUTPUT}"

# Create squashfs from rootfs
echo "==> Creating squashfs filesystem..."
if ! command -v mksquashfs &> /dev/null; then
    echo "ERROR: mksquashfs not found. Install squashfs-tools:"
    echo "  Ubuntu/Debian: sudo apt-get install squashfs-tools"
    exit 1
fi

sudo mksquashfs "${ROOTFS_DIR}" "${ISO_DIR}/live/filesystem.squashfs" \
    -comp xz \
    -Xbcj x86 \
    -b 1M \
    -noappend \
    -e boot

echo "  ✓ Squashfs created: $(du -h "${ISO_DIR}/live/filesystem.squashfs" | cut -f1)"

# Copy kernel from rootfs
echo "==> Copying kernel..."
cp "${ROOTFS_DIR}/boot/vmlinuz-sol" "${ISO_DIR}/boot/vmlinuz"

# Generate initramfs for live boot
echo "==> Generating initramfs..."
if ! command -v dracut &> /dev/null; then
    echo "WARNING: dracut not found, trying mkinitramfs..."
    if command -v mkinitramfs &> /dev/null; then
        sudo mkinitramfs -o "${ISO_DIR}/boot/initrd.img" "${KERNEL_VERSION}"
    else
        echo "ERROR: No initramfs generator found (dracut or mkinitramfs)"
        exit 1
    fi
else
    sudo dracut --force \
        --add "dmsquash-live" \
        --omit "plymouth" \
        --kver "${KERNEL_VERSION}" \
        "${ISO_DIR}/boot/initrd.img"
fi

echo "  ✓ Initramfs created: $(du -h "${ISO_DIR}/boot/initrd.img" | cut -f1)"

# Create GRUB configuration
echo "==> Configuring bootloader (GRUB)..."
cat > "${ISO_DIR}/boot/grub/grub.cfg" <<EOF
set default=0
set timeout=5

menuentry "SOL OS ${SOL_FULL_VERSION} (Live)" {
    linux /boot/vmlinuz boot=live quiet splash rootfstype=auto
    initrd /boot/initrd.img
}

menuentry "SOL OS ${SOL_FULL_VERSION} (Safe Mode)" {
    linux /boot/vmlinuz boot=live noapic acpi=off nomodeset
    initrd /boot/initrd.img
}

menuentry "SOL OS ${SOL_FULL_VERSION} (Debug)" {
    linux /boot/vmlinuz boot=live debug systemd.log_level=debug
    initrd /boot/initrd.img
}
EOF

# Install GRUB for BIOS boot
echo "==> Installing GRUB bootloader..."
if ! command -v grub-mkrescue &> /dev/null; then
    echo "ERROR: grub-mkrescue not found. Install grub-pc-bin and grub-efi-amd64-bin:"
    echo "  Ubuntu/Debian: sudo apt-get install grub-pc-bin grub-efi-amd64-bin xorriso"
    exit 1
fi

# Create EFI boot directory for UEFI support
mkdir -p "${ISO_DIR}/boot/grub/x86_64-efi"
mkdir -p "${ISO_DIR}/EFI/boot"

# Copy GRUB EFI modules if available
if [ -d /usr/lib/grub/x86_64-efi ]; then
    cp -r /usr/lib/grub/x86_64-efi/* "${ISO_DIR}/boot/grub/x86_64-efi/" 2>/dev/null || true
fi

# Create EFI boot image
if command -v grub-mkstandalone &> /dev/null; then
    grub-mkstandalone \
        --format=x86_64-efi \
        --output="${ISO_DIR}/EFI/boot/bootx64.efi" \
        --locales="" \
        --fonts="" \
        "boot/grub/grub.cfg=${ISO_DIR}/boot/grub/grub.cfg"
fi

# Generate ISO with hybrid BIOS/UEFI support
echo "==> Generating ISO image..."
grub-mkrescue \
    -o "${ISO_OUTPUT}/${ISO_NAME}" \
    "${ISO_DIR}" \
    -- \
    -volid "SOL_OS" \
    -joliet on \
    -joliet-long \
    -rational-rock

# Make ISO hybrid (bootable from USB)
if command -v isohybrid &> /dev/null; then
    echo "==> Making ISO hybrid..."
    isohybrid "${ISO_OUTPUT}/${ISO_NAME}" 2>/dev/null || true
fi

# Generate checksums
echo "==> Generating checksums..."
cd "${ISO_OUTPUT}"
sha256sum "${ISO_NAME}" > SHA256SUMS
md5sum "${ISO_NAME}" > MD5SUMS

# Display results
ISO_SIZE=$(du -h "${ISO_OUTPUT}/${ISO_NAME}" | cut -f1)
echo ""
echo "✓ ISO image created successfully!"
echo "  File: ${ISO_OUTPUT}/${ISO_NAME}"
echo "  Size: ${ISO_SIZE}"
echo "  SHA256: $(cat SHA256SUMS | cut -d' ' -f1)"
echo ""
echo "To test the ISO:"
echo "  qemu-system-x86_64 -m 2G -cdrom ${ISO_OUTPUT}/${ISO_NAME}"
echo ""
echo "To write to USB:"
echo "  sudo dd if=${ISO_OUTPUT}/${ISO_NAME} of=/dev/sdX bs=4M status=progress"
