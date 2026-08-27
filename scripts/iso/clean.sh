#!/bin/bash
# Clean build artifacts
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BUILD_DIR="${PROJECT_ROOT}/build"

echo "==> Cleaning SOL ISO build artifacts..."

# Clean kernel build
if [ -d "${BUILD_DIR}/kernel/linux-"* ]; then
    echo "  Removing kernel build directories..."
    rm -rf "${BUILD_DIR}"/kernel/linux-*/
fi

# Clean rootfs staging
if [ -d "${BUILD_DIR}/rootfs-staging" ]; then
    echo "  Removing rootfs staging..."
    sudo rm -rf "${BUILD_DIR}/rootfs-staging"
fi

# Clean platform/kernel/initramfs staging
for dir in platform-staging kernel-staging kernel-artifacts initramfs initramfs-staging esp esp-keys; do
    if [ -d "${BUILD_DIR}/${dir}" ]; then
        echo "  Removing ${dir}..."
        sudo rm -rf "${BUILD_DIR}/${dir}"
    fi
done
rm -f "${BUILD_DIR}/esp.img"

# Clean ISO staging
if [ -d "${BUILD_DIR}/iso-staging" ]; then
    echo "  Removing ISO staging..."
    rm -rf "${BUILD_DIR}/iso-staging"
fi

# Clean build markers
rm -f "${BUILD_DIR}/.rootfs-base-done"
rm -f "${BUILD_DIR}/kernel-version.txt"

# Optional: Clean generated ISOs
read -p "Remove generated ISOs? (y/N) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    if [ -d "${BUILD_DIR}/iso" ]; then
        echo "  Removing ISOs..."
        rm -f "${BUILD_DIR}"/iso/*.iso
        rm -f "${BUILD_DIR}"/iso/SHA256SUMS
        rm -f "${BUILD_DIR}"/iso/MD5SUMS
        rm -f "${BUILD_DIR}"/iso/RELEASE_NOTES.md
    fi
fi

# Optional: Clean cached kernel source
read -p "Remove cached kernel sources? (y/N) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo "  Removing kernel source archives..."
    rm -f "${BUILD_DIR}"/kernel/linux-*.tar.xz
fi

echo "✓ Clean complete"
