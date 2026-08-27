#!/bin/bash
# Build SOL platform components (compositor, shell, services, SDK) and stage
# them into build/platform-staging/.
#
# The platform is supervised by SOL's own init (sol-init + its `.daemon`
# files). No systemd units are generated: systemd is not part of the SOL
# runtime (see services/sol-init/README.md and ADR-0026).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILD_DIR="${PROJECT_ROOT}/build"
STAGING="${BUILD_DIR}/platform-staging"

echo "==> Building SOL platform components..."

cd "$PROJECT_ROOT"

# Build all workspace components in release mode. sol-boot and its UEFI
# artifact are built in create-iso.sh (they need the x86_64-unknown-uefi
# target and a release signing key). Set SKIP_CARGO_BUILD=1 to reuse an
# existing target/release (e.g. CI already built the workspace).
if [ "${SKIP_CARGO_BUILD:-0}" != "1" ]; then
    echo "==> Building workspace (release)..."
    cargo build --workspace --release --locked
fi

# Create a clean staging tree (mirrors the final root filesystem layout).
rm -rf "${STAGING}"
mkdir -p "${STAGING}/usr/bin"
mkdir -p "${STAGING}/usr/lib/sol"
mkdir -p "${STAGING}/usr/share/sol/daemons"
mkdir -p "${STAGING}/usr/share/dbus-1/services"
mkdir -p "${STAGING}/sbin"
mkdir -p "${STAGING}/etc/sol"

# ---------------------------------------------------------------------------
# Install binaries
# ---------------------------------------------------------------------------
echo "==> Installing SOL binaries..."

install_if_present() {
    local binary="$1"
    if [ -f "target/release/${binary}" ]; then
        install -Dm755 "target/release/${binary}" "${STAGING}/usr/bin/${binary}"
        echo "  ✓ ${binary}"
    else
        echo "  - ${binary} (not built)"
    fi
}

# Core session
install_if_present sol-compositor
install_if_present sol-shell
install_if_present sol-init
install_if_present lyra

# Services (every service in services/ with a binary is shipped and supervised
# by sol-init). sol-ime is library-only today (fcitx5 bridge deferred) and
# sol-scheduler is a support library used by sol-init/sol-session, not a
# standalone daemon.
for service in \
    sol-settingsd sol-notificationd sol-portal \
    sol-networkd sol-audiod sol-ntpd sol-diagnostics sol-deviced; do
    install_if_present "${service}"
done

# Apps
for app in sol-files sol-terminal sol-settings sol-installer; do
    install_if_present "${app}"
done

# ---------------------------------------------------------------------------
# Install sol-init daemon definitions
#
# sol-init is the SOL daemon supervisor: it loads /usr/share/sol/daemons/*.daemon
# in dependency order and restarts daemons per their restart policy.
# Excluded from the system set:
#   - example-app-daemon.daemon  (third-party template, not a SOL daemon)
#   - sol-audio.daemon           (PipeWire wrapper; PipeWire ships later)
# ---------------------------------------------------------------------------
echo "==> Installing sol-init daemon definitions..."
install -Dm644 services/sol-init/daemons/org.freedesktop.DBus.daemon \
    "${STAGING}/usr/share/sol/daemons/org.freedesktop.DBus.daemon"
for daemon in \
    sol-compositor.daemon sol-shell.daemon sol-portal.daemon sol-networkd.daemon \
    sol-settingsd.daemon sol-notificationd.daemon \
    sol-audiod.daemon sol-ntpd.daemon sol-diagnostics.daemon sol-deviced.daemon; do
    if [ -f "services/sol-init/daemons/${daemon}" ]; then
        install -Dm644 "services/sol-init/daemons/${daemon}" \
            "${STAGING}/usr/share/sol/daemons/${daemon}"
        echo "  ✓ ${daemon}"
    else
        echo "  - ${daemon} (missing)"
    fi
done

# ---------------------------------------------------------------------------
# Daemon launchers for services that need CLI flags
#
# sol-init's daemon `exec` is a single program path (no argument vector), so
# services that must run with flags get a thin launcher. The installed daemon
# definitions below are rewritten to point at the launcher.
# ---------------------------------------------------------------------------
echo "==> Installing daemon launchers (flag wrappers)..."
mkdir -p "${STAGING}/usr/lib/sol/runtime"
for wrapper in sol-settingsd:--dbus sol-notificationd:--dbus sol-portal:--dbus; do
    name=${wrapper%%:*}
    flag=${wrapper#*:}
    cat > "${STAGING}/usr/lib/sol/runtime/${name}" <<EOF
#!/bin/sh
exec /usr/bin/${name} ${flag} "\$@"
EOF
    chmod 0755 "${STAGING}/usr/lib/sol/runtime/${name}"
    sed -i "s|^exec = .*|exec = \"/usr/lib/sol/runtime/${name}\"|" \
        "${STAGING}/usr/share/sol/daemons/${name}.daemon"
    echo "  ✓ ${name} -> /usr/lib/sol/runtime/${name}"
done

# ---------------------------------------------------------------------------
# Install D-Bus activation files (on-demand activation via sol-init --activate)
# ---------------------------------------------------------------------------
echo "==> Installing D-Bus activation files..."
for service in org.sol.Settings.service org.sol.Notifications.service; do
    install -Dm644 "services/sol-init/daemons/${service}" \
        "${STAGING}/usr/share/dbus-1/services/${service}"
    echo "  ✓ ${service}"
done

# ---------------------------------------------------------------------------
# Install SDK libraries
# ---------------------------------------------------------------------------
echo "==> Installing SDK libraries..."
find target/release -maxdepth 1 \( -name "libsol_*.so" -o -name "libsol*.rlib" \) | while read -r lib; do
    install -Dm644 "$lib" "${STAGING}/usr/lib/sol/$(basename "$lib")"
done

cat > "${STAGING}/usr/lib/sol/sol-dbus" <<'EOF'
#!/bin/sh
# SOL session D-Bus launcher (invoked by sol-init as a daemon)
exec /usr/bin/dbus-daemon --session \
    --address="${DBUS_SESSION_BUS_ADDRESS:-unix:path=/run/sol-session.sock}" \
    --nofork --nopidfile
EOF
chmod 0755 "${STAGING}/usr/lib/sol/sol-dbus"

# ---------------------------------------------------------------------------
# Install /sbin/init - the SOL userspace bringup wrapper.
#
# PID 1 is SOL-owned: this wrapper prepares the minimal runtime environment
# (tmpfs on /run and /tmp, the session D-Bus address, runtime dir) and hands
# control to sol-init, which then supervises every SOL daemon (including the
# D-Bus session bus) in dependency order. No systemd is involved anywhere in
# the boot path.
# ---------------------------------------------------------------------------
echo "==> Installing /sbin/init (SOL userspace bringup)..."
cat > "${STAGING}/sbin/init" <<'EOF'
#!/bin/sh
# SOL userspace bringup (PID 1)
#
# Minimal environment preparation before sol-init takes over as the daemon
# supervisor. This is the only non-SOL code in the runtime init path.

PATH=/usr/bin:/usr/sbin:/bin:/sbin
export PATH

# Runtime tmpfs (kernel devtmpfs/proc/sys are mounted by the initramfs)
mount -t tmpfs tmpfs /run 2>/dev/null || true
mount -t tmpfs tmpfs /tmp 2>/dev/null || true

# The runtime directory for the desktop session
export XDG_RUNTIME_DIR="/run/user/$(id -u 2>/dev/null || echo 0)"
mkdir -p "${XDG_RUNTIME_DIR}" /run/dbus
chmod 0700 "${XDG_RUNTIME_DIR}"
chmod 0755 /run/dbus

# Session D-Bus address shared with every sol-init daemon through the
# environment; the actual daemon is started by sol-init (org.freedesktop.DBus
# daemon definition) in dependency order.
export DBUS_SESSION_BUS_ADDRESS="unix:path=/run/sol-session.sock"

# Hand over to the SOL daemon supervisor
exec /usr/bin/sol-init
EOF
chmod 0755 "${STAGING}/sbin/init"
# sol-init is the conventional init binary as well, for direct invocation
ln -sf ../../usr/bin/sol-init "${STAGING}/sbin/sol-init" 2>/dev/null || true
ln -sf ../../usr/bin/sol-init "${STAGING}/usr/sbin/sol-init" 2>/dev/null || true

echo "✓ SOL platform staged in ${STAGING}"