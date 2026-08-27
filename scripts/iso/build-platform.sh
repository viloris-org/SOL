#!/bin/bash
# Build SOL platform components (compositor, shell, services, SDK)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILD_DIR="${PROJECT_ROOT}/build"
ROOTFS_DIR="${BUILD_DIR}/rootfs-staging"

echo "==> Building SOL platform components..."

cd "$PROJECT_ROOT"

# Build all workspace components in release mode
echo "==> Building workspace (release)..."
cargo build --workspace --release --locked

# Create directory structure
echo "==> Creating rootfs directories..."
mkdir -p "${ROOTFS_DIR}/usr/bin"
mkdir -p "${ROOTFS_DIR}/usr/lib/sol"
mkdir -p "${ROOTFS_DIR}/usr/share/sol"
mkdir -p "${ROOTFS_DIR}/etc/sol"
mkdir -p "${ROOTFS_DIR}/lib/systemd/system"
mkdir -p "${ROOTFS_DIR}/etc/dbus-1/system.d"

# Install binaries
echo "==> Installing SOL binaries..."

# Core compositor
if [ -f "target/release/sol-compositor" ]; then
    install -Dm755 target/release/sol-compositor "${ROOTFS_DIR}/usr/bin/sol-compositor"
    echo "  ✓ sol-compositor"
fi

# Shell
if [ -f "target/release/sol-shell" ]; then
    install -Dm755 target/release/sol-shell "${ROOTFS_DIR}/usr/bin/sol-shell"
    echo "  ✓ sol-shell"
fi

# Services
for service in sol-settingsd sol-notificationd sol-portal sol-ime; do
    if [ -f "target/release/${service}" ]; then
        install -Dm755 "target/release/${service}" "${ROOTFS_DIR}/usr/bin/${service}"
        echo "  ✓ ${service}"
    fi
done

# Apps
for app in sol-files sol-terminal sol-settings; do
    if [ -f "target/release/${app}" ]; then
        install -Dm755 "target/release/${app}" "${ROOTFS_DIR}/usr/bin/${app}"
        echo "  ✓ ${app}"
    fi
done

# Install SDK libraries
echo "==> Installing SDK libraries..."
find target/release -maxdepth 1 -name "libsol_*.so" -o -name "libsol*.rlib" | while read -r lib; do
    install -Dm644 "$lib" "${ROOTFS_DIR}/usr/lib/sol/$(basename "$lib")"
done

# Install systemd service files
echo "==> Installing systemd units..."

# Compositor service
cat > "${ROOTFS_DIR}/lib/systemd/system/sol-compositor.service" <<'EOF'
[Unit]
Description=SOL Compositor
Documentation=https://github.com/solOS/sol
After=systemd-user-sessions.service
Before=graphical.target
Wants=dbus.service

[Service]
Type=notify
ExecStart=/usr/bin/sol-compositor
Restart=on-failure
RestartSec=5
StandardOutput=journal
StandardError=journal
TimeoutStartSec=30

# Security hardening
PrivateTmp=yes
ProtectSystem=strict
ProtectHome=yes
NoNewPrivileges=yes
ReadWritePaths=/run /tmp

[Install]
WantedBy=graphical.target
EOF

# Shell service
cat > "${ROOTFS_DIR}/lib/systemd/system/sol-shell.service" <<'EOF'
[Unit]
Description=SOL Shell
Documentation=https://github.com/solOS/sol
After=sol-compositor.service
PartOf=graphical.target
Requires=sol-compositor.service

[Service]
Type=simple
ExecStart=/usr/bin/sol-shell
Restart=on-failure
RestartSec=3
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=graphical.target
EOF

# Settings daemon
cat > "${ROOTFS_DIR}/lib/systemd/system/sol-settingsd.service" <<'EOF'
[Unit]
Description=SOL Settings Daemon
Documentation=https://github.com/solOS/sol
After=dbus.service

[Service]
Type=dbus
BusName=org.sol.Settings
ExecStart=/usr/bin/sol-settingsd
Restart=on-failure

[Install]
WantedBy=multi-user.target
EOF

# Notification daemon
cat > "${ROOTFS_DIR}/lib/systemd/system/sol-notificationd.service" <<'EOF'
[Unit]
Description=SOL Notification Daemon
Documentation=https://github.com/solOS/sol
After=dbus.service

[Service]
Type=dbus
BusName=org.freedesktop.Notifications
ExecStart=/usr/bin/sol-notificationd
Restart=on-failure

[Install]
WantedBy=multi-user.target
EOF

echo "✓ SOL platform build complete"
