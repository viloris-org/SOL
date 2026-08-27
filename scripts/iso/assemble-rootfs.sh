#!/bin/bash
# Assemble the SOL root filesystem.
#
# The base is a minimal Debian minbase WITHOUT systemd: SOL owns the init
# path (/sbin/init -> sol-init) and the daemon supervision (sol-init's
# .daemon files). The kernel modules and platform components staged by
# build-kernel.sh / build-platform.sh are merged in afterward.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILD_DIR="${PROJECT_ROOT}/build"
ROOTFS_DIR="${BUILD_DIR}/rootfs-staging"
KERNEL_STAGING="${BUILD_DIR}/kernel-staging"
PLATFORM_STAGING="${BUILD_DIR}/platform-staging"

echo "==> Assembling SOL root filesystem..."

# ---------------------------------------------------------------------------
# Base system (Debian bookworm minbase, no systemd in the runtime)
# ---------------------------------------------------------------------------
if [ ! -f "${BUILD_DIR}/.rootfs-base-done" ]; then
    echo "==> Installing base system (Debian bookworm minimal, systemd-free)..."

    if ! command -v debootstrap &> /dev/null; then
        echo "ERROR: debootstrap not found. Install it first:"
        echo "  Ubuntu/Debian: sudo apt-get install debootstrap"
        exit 1
    fi
    if [ -d "${ROOTFS_DIR}" ] && [ -n "$(ls -A "${ROOTFS_DIR}" 2>/dev/null)" ]; then
        echo "ERROR: ${ROOTFS_DIR} exists and is not empty; debootstrap needs an"
        echo "       empty target. Remove it first: rm -rf ${ROOTFS_DIR}"
        exit 1
    fi

    sudo debootstrap \
        --variant=minbase \
        --include=dbus,util-linux,coreutils,bash,passwd,kmod,ca-certificates,busybox-static \
        --exclude=ifupdown,isc-dhcp-client,isc-dhcp-common,systemd,systemd-sysv,udev \
        bookworm \
        "${ROOTFS_DIR}" \
        http://deb.debian.org/debian

    touch "${BUILD_DIR}/.rootfs-base-done"
    echo "  ✓ Base system installed (minbase, systemd-free)"
else
    echo "  ✓ Using cached base system"
fi

# ---------------------------------------------------------------------------
# Merge SOL components into the rootfs
# ---------------------------------------------------------------------------
if [ -d "${KERNEL_STAGING}" ]; then
    echo "==> Merging kernel staging..."
    sudo rsync -a "${KERNEL_STAGING}/" "${ROOTFS_DIR}/"
fi
if [ -d "${PLATFORM_STAGING}" ]; then
    echo "==> Merging platform staging..."
    sudo rsync -a "${PLATFORM_STAGING}/" "${ROOTFS_DIR}/"
fi

# ---------------------------------------------------------------------------
# Configure the system
# ---------------------------------------------------------------------------
echo "==> Configuring system..."

# Hostname
echo "sol" | sudo tee "${ROOTFS_DIR}/etc/hostname" > /dev/null

# Hosts file
sudo tee "${ROOTFS_DIR}/etc/hosts" > /dev/null <<'EOF'
127.0.0.1   localhost
127.0.1.1   sol
::1         localhost ip6-localhost ip6-loopback
ff02::1     ip6-allnodes
ff02::2     ip6-allrouters
EOF

# fstab (informational: the initramfs mounts the live root; /run and /tmp are
# mounted by the SOL PID1 bringup before sol-init starts)
sudo tee "${ROOTFS_DIR}/etc/fstab" > /dev/null <<'EOF'
# SOL OS filesystem table
tmpfs      /tmp       tmpfs   defaults,noatime,mode=1777  0 0
tmpfs      /run       tmpfs   defaults,noatime,mode=0755  0 0
EOF

# Environment for the desktop session
sudo tee "${ROOTFS_DIR}/etc/profile.d/sol.sh" > /dev/null <<'EOF'
# SOL environment variables
export XDG_RUNTIME_DIR="/run/user/$(id -u)"
export SOL_COMPOSITOR_SOCKET="${XDG_RUNTIME_DIR}/sol-compositor.sock"

if [ ! -d "$XDG_RUNTIME_DIR" ]; then
    mkdir -p "$XDG_RUNTIME_DIR"
    chmod 0700 "$XDG_RUNTIME_DIR"
fi
EOF

# Release file
KERNEL_VERSION=$(cat "${BUILD_DIR}/kernel-version.txt" 2>/dev/null || echo "unknown")
SOL_VERSION=$(cat "${PROJECT_ROOT}/VERSION" 2>/dev/null || \
    git -C "$PROJECT_ROOT" describe --tags --always 2>/dev/null || echo "dev")
SOL_CODENAME=$(cat "${PROJECT_ROOT}/CODENAME" 2>/dev/null || echo "")
sudo tee "${ROOTFS_DIR}/etc/sol-release" > /dev/null <<EOF
SOL_VERSION=${SOL_VERSION}
SOL_CODENAME=${SOL_CODENAME}
SOL_KERNEL=${KERNEL_VERSION}
SOL_BUILD_DATE=$(date -u +"%Y-%m-%d %H:%M:%S UTC")
EOF

# ---------------------------------------------------------------------------
# Clean up
# ---------------------------------------------------------------------------
echo "==> Cleaning up..."
sudo rm -rf "${ROOTFS_DIR}"/var/cache/apt/archives/*.deb
sudo rm -rf "${ROOTFS_DIR}"/tmp/*
sudo rm -rf "${ROOTFS_DIR}"/var/tmp/*

# Run tmpfs mounts are created by /sbin/init at boot; ensure directories exist
sudo chmod 1777 "${ROOTFS_DIR}/tmp"
sudo chmod 0755 "${ROOTFS_DIR}/run"

echo "✓ Root filesystem assembled"
if [ -n "$SOL_CODENAME" ]; then
    echo "  Version: ${SOL_VERSION} (${SOL_CODENAME})"
else
    echo "  Version: ${SOL_VERSION}"
fi
echo "  Kernel: ${KERNEL_VERSION}"
echo "  Init: sol-init (systemd-free)"