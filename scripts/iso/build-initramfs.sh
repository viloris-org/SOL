#!/bin/bash
# Build the minimal SOL initramfs.
#
# The initramfs is a single static busybox plus a small init script that:
#   1. mounts the kernel pseudo-filesystems,
#   2. locates the live medium (/live/filesystem.squashfs on the ISO),
#   3. mounts it read-only and stacks a writable tmpfs overlay,
#   4. switch_roots into the SOL root filesystem and executes /sbin/init
#      (the SOL userspace bringup that hands control to sol-init).
#
# No systemd, no dracut: the entire early-userspace path is SOL-owned.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILD_DIR="${PROJECT_ROOT}/build"
STAGING="${BUILD_DIR}/initramfs-staging"
OUTPUT_DIR="${BUILD_DIR}/initramfs"
ROOTFS_DIR="${BUILD_DIR}/rootfs-staging"

echo "==> Building SOL initramfs..."

# Locate a static busybox
BUSYBOX="${BUSYBOX:-}"
if [ -z "$BUSYBOX" ]; then
    for candidate in \
        "${ROOTFS_DIR}/bin/busybox" \
        "${ROOTFS_DIR}/usr/bin/busybox" \
        /bin/busybox; do
        if [ -x "$candidate" ]; then
            BUSYBOX="$candidate"
            break
        fi
    done
fi
if [ -z "$BUSYBOX" ]; then
    echo "ERROR: static busybox not found. Install busybox-static in the base"
    echo "       filesystem (debootstrap --include=busybox-static) or pass"
    echo "       BUSYBOX=/path/to/busybox to this script."
    exit 1
fi
echo "  Using busybox: ${BUSYBOX}"

rm -rf "${STAGING}"
mkdir -p "${STAGING}/bin" "${STAGING}/sbin" "${STAGING}/proc" \
    "${STAGING}/sys" "${STAGING}/dev" "${STAGING}/mnt" "${STAGING}/sysroot"

cp -L "${BUSYBOX}" "${STAGING}/bin/busybox"
chmod 0755 "${STAGING}/bin/busybox"

# Install applet symlinks NOW so /bin/sh (and friends) exist when the kernel
# execs /init - the shebang must resolve before the script can run.
"${STAGING}/bin/busybox" --install -s \
    "${STAGING}/bin" "${STAGING}/sbin" "${STAGING}/usr/bin" "${STAGING}/usr/sbin" \
    >/dev/null 2>&1 || true

# Minimal init: the SOL live bootstrapper
cat > "${STAGING}/init" <<'EOF'
#!/bin/sh
# SOL initramfs init (busybox)

BUSYBOX=/bin/busybox
$BUSYBOX --install -s /bin /sbin /usr/bin /usr/sbin 2>/dev/null
export PATH=/bin:/sbin:/usr/bin:/usr/sbin

mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev
mkdir -p /dev/pts
mount -t devpts devpts /dev/pts 2>/dev/null

echo "SOL: locating live medium..."

LIVE_DIR=""
for dev in /dev/sr[0-9]* /dev/cdrom* /dev/sd[a-z]* /dev/vd[a-z]* /dev/hd[a-z]*; do
    [ -e "$dev" ] || continue
    mkdir -p /mnt/live
    if mount -t iso9660 -o ro "$dev" /mnt/live 2>/dev/null; then
        if [ -f /mnt/live/live/filesystem.squashfs ]; then
            LIVE_DIR=/mnt/live
            echo "SOL: live medium found at $dev"
            break
        fi
        umount /mnt/live 2>/dev/null
    fi
done

if [ -z "$LIVE_DIR" ]; then
    echo "SOL: ERROR - live medium with /live/filesystem.squashfs not found" >/dev/console
    exec /bin/sh
fi

echo "SOL: mounting root filesystem (squashfs + tmpfs overlay)..."
mkdir -p /mnt/squashfs /mnt/overlay /sysroot
if ! mount -t squashfs -o loop,ro "${LIVE_DIR}/live/filesystem.squashfs" /mnt/squashfs \
    2>/dev/null && ! mount -t squashfs -o ro "${LIVE_DIR}/live/filesystem.squashfs" /mnt/squashfs \
    2>/dev/null; then
    echo "SOL: ERROR - failed to mount squashfs root" >/dev/console
    exec /bin/sh
fi

mount -t tmpfs tmpfs /mnt/overlay
mkdir -p /mnt/overlay/upper /mnt/overlay/work
if ! mount -t overlay overlay \
    -o lowerdir=/mnt/squashfs,upperdir=/mnt/overlay/upper,workdir=/mnt/overlay/work \
    /sysroot; then
    echo "SOL: ERROR - failed to mount overlay root" >/dev/console
    exec /bin/sh
fi

# Keep the live medium mounted for diagnostics inside the booted system
mkdir -p /sysroot/live
mount --move "$LIVE_DIR" /sysroot/live 2>/dev/null || true

echo "SOL: switching root..."
if command -v switch_root >/dev/null 2>&1; then
    exec switch_root /sysroot /sbin/init
fi

# Fallback for busybox builds without switch_root
mount --move /proc /sysroot/proc 2>/dev/null || true
mount --move /sys /sysroot/sys 2>/dev/null || true
mount --move /dev /sysroot/dev 2>/dev/null || true
exec chroot /sysroot /sbin/init
EOF
chmod 0755 "${STAGING}/init"

# Assemble compressed cpio archive (gzip: CONFIG_RD_GZIP in the SOL kernel)
mkdir -p "${OUTPUT_DIR}"
(
    cd "${STAGING}"
    find . -print0 | sort -z | cpio --null -o --format=newc 2>/dev/null \
        | gzip -9 > "${OUTPUT_DIR}/initramfs.cpio.gz"
)

echo "  ✓ Initramfs: ${OUTPUT_DIR}/initramfs.cpio.gz ($(du -h "${OUTPUT_DIR}/initramfs.cpio.gz" | cut -f1))"