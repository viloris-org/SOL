#!/bin/bash
# Build latest stable Linux kernel for SOL OS
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILD_DIR="${PROJECT_ROOT}/build"
KERNEL_BUILD_DIR="${BUILD_DIR}/kernel"
ROOTFS_DIR="${BUILD_DIR}/rootfs-staging"

echo "==> Fetching latest stable kernel version..."
KERNEL_VERSION=$(curl -s https://www.kernel.org/releases.json | jq -r '.latest_stable.version')
KERNEL_MAJOR=$(echo "$KERNEL_VERSION" | cut -d. -f1)
KERNEL_URL="https://cdn.kernel.org/pub/linux/kernel/v${KERNEL_MAJOR}.x/linux-${KERNEL_VERSION}.tar.xz"

echo "==> Latest stable kernel: ${KERNEL_VERSION}"

cd "$KERNEL_BUILD_DIR"

# Download kernel if not cached
if [ ! -f "linux-${KERNEL_VERSION}.tar.xz" ]; then
    echo "==> Downloading kernel source..."
    curl -L -o "linux-${KERNEL_VERSION}.tar.xz" "$KERNEL_URL"
else
    echo "==> Using cached kernel source"
fi

# Extract
if [ ! -d "linux-${KERNEL_VERSION}" ]; then
    echo "==> Extracting kernel..."
    tar xf "linux-${KERNEL_VERSION}.tar.xz"
fi

cd "linux-${KERNEL_VERSION}"

# Apply SOL kernel config
echo "==> Configuring kernel..."
if [ -f "${BUILD_DIR}/kernel/sol.config" ]; then
    cp "${BUILD_DIR}/kernel/sol.config" .config
    make olddefconfig
else
    echo "WARNING: No sol.config found, using defconfig"
    make defconfig
    
    # Enable essential SOL features
    scripts/config --enable DRM
    scripts/config --enable DRM_KMS_HELPER
    scripts/config --enable FB
    scripts/config --enable FRAMEBUFFER_CONSOLE
    scripts/config --enable INPUT_EVDEV
    scripts/config --enable SQUASHFS
    scripts/config --enable OVERLAY_FS
    scripts/config --enable TMPFS
    scripts/config --enable TMPFS_POSIX_ACL
    scripts/config --enable TMPFS_XATTR
    
    # Disable unnecessary features for size
    scripts/config --disable WIRELESS
    scripts/config --disable WLAN
    scripts/config --disable BT
    
    make olddefconfig
fi

# Build kernel
echo "==> Building kernel with $(nproc) cores..."
make -j$(nproc) bzImage modules

# Install modules
echo "==> Installing kernel modules..."
mkdir -p "${ROOTFS_DIR}"
INSTALL_MOD_PATH="${ROOTFS_DIR}" make modules_install

# Install kernel image
echo "==> Installing kernel image..."
mkdir -p "${ROOTFS_DIR}/boot"
cp arch/x86/boot/bzImage "${ROOTFS_DIR}/boot/vmlinuz-${KERNEL_VERSION}"
ln -sf "vmlinuz-${KERNEL_VERSION}" "${ROOTFS_DIR}/boot/vmlinuz-sol"

# Save kernel version for later stages
echo "$KERNEL_VERSION" > "${BUILD_DIR}/kernel-version.txt"

echo "✓ Kernel build complete: ${KERNEL_VERSION}"
