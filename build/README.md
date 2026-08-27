# SOL ISO Build System

This directory contains the complete CI/CD infrastructure for building bootable
SOL OS ISO images.

## Architecture

The ISO build pipeline is split into 5 distinct stages:

```
1. Kernel Build    → Latest stable Linux kernel with SOL config (EFI stub,
                     initramfs, ISO9660/squashfs/overlay support)
2. Platform Build  → ALL SOL services + sol-init daemon definitions
3. RootFS Assembly → Debian minbase (systemd-free) + kernel/platform merge
4. ISO Creation    → busybox initramfs + sol-boot UEFI ESP + xorriso ISO
5. Boot Testing    → QEMU + OVMF smoke test
```

Boot chain (no GRUB, no systemd, no dracut):

```
OVMF (UEFI)
 └─ EFI/BOOT/BOOTX64.EFI            = sol-boot (signed UEFI application)
     └─ EFI/SOL/slots/A/system.efi  = UKI (kernel, CONFIG_EFI_STUB)
         └─ kernel loads EFI/SOL/slots/A/initrd.img (busybox initramfs)
             └─ initramfs mounts /live/filesystem.squashfs (overlay root)
                 └─ /sbin/init → sol-init (SOL daemon supervisor)
                     ├─ dbus          (session D-Bus)
                     ├─ sol-compositor
                     ├─ sol-shell
                     ├─ sol-networkd / sol-audiod / sol-ntpd / sol-deviced
                     └─ sol-settingsd / sol-notificationd / sol-portal
                        (D-Bus activated on demand)
```

## Quick Start

### Local Build

```bash
# Build complete ISO (all stages)
./scripts/build-iso.sh

# Or run stages individually
./scripts/iso/build-kernel.sh       # Stage 1: Kernel
./scripts/iso/build-platform.sh     # Stage 2: SOL components
./scripts/iso/assemble-rootfs.sh    # Stage 3: Filesystem
./scripts/iso/create-iso.sh         # Stage 4: ISO image (builds initramfs
                                    #          and sol-boot itself)
./scripts/iso/test-boot.sh          # Stage 5: UEFI boot test (QEMU + OVMF)
```

### CI/CD

The GitHub Actions workflow `.github/workflows/build-iso.yml` automatically
builds ISOs on:
- Git tags (`v*`)
- Manual workflow dispatch

```bash
# Trigger build via tag
git tag v0.1.0
git push origin v0.1.0

# Or use GitHub Actions UI for manual trigger
```

## Directory Structure

```
build/
├── kernel/
│   ├── sol.config              # Kernel configuration (tracked)
│   └── linux-*.tar.xz          # Cached kernel source
├── kernel-staging/             # Kernel modules + bzImage (merged into rootfs)
├── kernel-artifacts/           # Bare bzImage for UKI assembly
├── platform-staging/           # SOL binaries, daemons, /sbin/init
├── initramfs/                  # busybox initramfs (initramfs.cpio.gz)
├── initramfs-staging/          # initramfs build tree
├── esp/                        # EFI System Partition staging (EFI/SOL/...)
├── esp.img                     # FAT32 image used as the El Torito boot entry
├── rootfs-staging/             # Assembled root filesystem
├── iso-staging/                # ISO build staging area
└── iso/                        # Final ISO + checksums
```

## Output

After successful build:
- **ISO**: `build/iso/sol-{version}-x86_64.iso` (UEFI-only)
- **Checksums**: `SHA256SUMS`, `MD5SUMS`
- **Bootable**: UEFI (via sol-boot, no BIOS/GRUB legacy boot)

## Testing

```bash
# QEMU (needs OVMF for UEFI)
qemu-system-x86_64 -m 2G -smp 2 \
  -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE.fd \
  -drive if=pflash,format=raw,file=/usr/share/OVMF/OVMF_VARS.fd \
  -cdrom build/iso/sol-*.iso

# USB (UEFI)
sudo dd if=build/iso/sol-*.iso of=/dev/sdX bs=4M status=progress
```

## Why no GRUB / systemd / dracut?

SOL is a Linux Family OS, not a distribution. Its runtime is SOL-owned:

- **bootloader** — `sol-boot` (boot/), a signed UEFI application with A/B
  slots, bounded trials, and verified UKI transfer (ADR-0026);
- **init** — `sol-init` (services/sol-init), the SOL daemon supervisor with
  dependency ordering, restart policies, and `.daemon` definitions;
- **initramfs** — a small busybox initramfs that locates the live squashfs
  and hands control to `/sbin/init`;
- **services** — every SOL service ships and is supervised by sol-init.

The kernel still carries CONFIG_CMDLINE with the initrd path because sol-boot
transfers control without a command line (anonymous buffer LoadImage); the
initrd file itself lives on the SOL ESP next to the UKI.