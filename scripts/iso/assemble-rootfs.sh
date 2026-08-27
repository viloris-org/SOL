#!/bin/bash
# Assemble the SOL root filesystem
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILD_DIR="${PROJECT_ROOT}/build"
ROOTFS_DIR="${BUILD_DIR}/rootfs-staging"
ROOTFS_OVERLAY="${BUILD_DIR}/rootfs"

echo "==> Assembling SOL root filesystem..."

# Create base directory structure
echo "==> Creating directory structure..."
mkdir -p "${ROOTFS_DIR}"/{boot,dev,proc,sys,tmp,run,home,root}
mkdir -p "${ROOTFS_DIR}"/usr/{bin,sbin,lib,lib64,share}
mkdir -p "${ROOTFS_DIR}"/etc/{systemd/system,dbus-1/system.d,xdg,sol}
mkdir -p "${ROOTFS_DIR}"/var/{log,cache,lib}

# Install base system using debootstrap (minimal Debian base)
if [ ! -f "${BUILD_DIR}/.rootfs-base-done" ]; then
    echo "==> Installing base system (Debian bookworm minimal)..."
    
    # Check if debootstrap is available
    if ! command -v debootstrap &> /dev/null; then
        echo "ERROR: debootstrap not found. Install it first:"
        echo "  Ubuntu/Debian: sudo apt-get install debootstrap"
        exit 1
    fi
    
    sudo debootstrap \
        --variant=minbase \
        --include=systemd,dbus,udev,kmod,util-linux,coreutils,bash,ca-certificates \
        --exclude=ifupdown,isc-dhcp-client,isc-dhcp-common \
        bookworm \
        "${ROOTFS_DIR}" \
        http://deb.debian.org/debian
    
    touch "${BUILD_DIR}/.rootfs-base-done"
    echo "  ✓ Base system installed"
else
    echo "  ✓ Using cached base system"
fi

# Apply SOL overlay if it exists
if [ -d "${ROOTFS_OVERLAY}" ]; then
    echo "==> Applying SOL overlay..."
    sudo rsync -a "${ROOTFS_OVERLAY}/" "${ROOTFS_DIR}/"
fi

# Configure system
echo "==> Configuring system..."

# Set hostname
echo "sol" | sudo tee "${ROOTFS_DIR}/etc/hostname" > /dev/null

# Configure hosts file
sudo tee "${ROOTFS_DIR}/etc/hosts" > /dev/null <<'EOF'
127.0.0.1   localhost
127.0.1.1   sol
::1         localhost ip6-localhost ip6-loopback
ff02::1     ip6-allnodes
ff02::2     ip6-allrouters
EOF

# Configure fstab
sudo tee "${ROOTFS_DIR}/etc/fstab" > /dev/null <<'EOF'
# SOL OS filesystem table
tmpfs      /tmp       tmpfs   defaults,noatime,mode=1777  0 0
tmpfs      /run       tmpfs   defaults,noatime,mode=0755  0 0
overlay    /          overlay defaults                    0 0
EOF

# Set default systemd target to graphical
sudo ln -sf /lib/systemd/system/graphical.target \
    "${ROOTFS_DIR}/etc/systemd/system/default.target"

# Enable SOL services
echo "==> Enabling SOL services..."
for service in sol-compositor sol-shell sol-settingsd sol-notificationd; do
    if [ -f "${ROOTFS_DIR}/lib/systemd/system/${service}.service" ]; then
        sudo ln -sf "/lib/systemd/system/${service}.service" \
            "${ROOTFS_DIR}/etc/systemd/system/graphical.target.wants/${service}.service"
        echo "  ✓ Enabled ${service}"
    fi
done

# Create SOL user (default user)
echo "==> Creating SOL user..."
sudo chroot "${ROOTFS_DIR}" useradd -m -s /bin/bash -G audio,video,input sol || true
sudo chroot "${ROOTFS_DIR}" passwd -d sol  # No password for live boot

# Set up autologin
sudo mkdir -p "${ROOTFS_DIR}/etc/systemd/system/getty@tty1.service.d"
sudo tee "${ROOTFS_DIR}/etc/systemd/system/getty@tty1.service.d/autologin.conf" > /dev/null <<'EOF'
[Service]
ExecStart=
ExecStart=-/sbin/agetty --autologin sol --noclear %I $TERM
EOF

# Configure environment
sudo tee "${ROOTFS_DIR}/etc/profile.d/sol.sh" > /dev/null <<'EOF'
# SOL environment variables
export XDG_RUNTIME_DIR="/run/user/$(id -u)"
export SOL_COMPOSITOR_SOCKET="${XDG_RUNTIME_DIR}/sol-compositor.sock"

# Ensure runtime directory exists
if [ ! -d "$XDG_RUNTIME_DIR" ]; then
    mkdir -p "$XDG_RUNTIME_DIR"
    chmod 0700 "$XDG_RUNTIME_DIR"
fi
EOF

# Create version file
KERNEL_VERSION=$(cat "${BUILD_DIR}/kernel-version.txt" 2>/dev/null || echo "unknown")

if [ -f "${PROJECT_ROOT}/VERSION" ]; then
    SOL_VERSION=$(cat "${PROJECT_ROOT}/VERSION")
else
    SOL_VERSION=$(git -C "$PROJECT_ROOT" describe --tags --always 2>/dev/null || echo "dev")
fi

if [ -f "${PROJECT_ROOT}/CODENAME" ]; then
    SOL_CODENAME=$(cat "${PROJECT_ROOT}/CODENAME")
else
    SOL_CODENAME=""
fi

sudo tee "${ROOTFS_DIR}/etc/sol-release" > /dev/null <<EOF
SOL_VERSION=${SOL_VERSION}
SOL_CODENAME=${SOL_CODENAME}
SOL_KERNEL=${KERNEL_VERSION}
SOL_BUILD_DATE=$(date -u +"%Y-%m-%d %H:%M:%S UTC")
EOF

# Clean up
echo "==> Cleaning up..."
sudo rm -rf "${ROOTFS_DIR}"/var/cache/apt/archives/*.deb
sudo rm -rf "${ROOTFS_DIR}"/tmp/*
sudo rm -rf "${ROOTFS_DIR}"/var/tmp/*

# Set permissions
sudo chmod 1777 "${ROOTFS_DIR}/tmp"
sudo chmod 0755 "${ROOTFS_DIR}/run"

echo "✓ Root filesystem assembled"
if [ -n "$SOL_CODENAME" ]; then
    echo "  Version: ${SOL_VERSION} (${SOL_CODENAME})"
else
    echo "  Version: ${SOL_VERSION}"
fi
echo "  Kernel: ${KERNEL_VERSION}"
