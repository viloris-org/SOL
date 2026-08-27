# SOL ISO Build System

Complete CI/CD pipeline for building bootable SOL OS ISO images with the
latest stable kernel.

## Quick Start

```bash
# Build everything
./scripts/build-iso.sh

# Or run individual stages
./scripts/iso/build-kernel.sh       # Build kernel (EFI stub + initramfs + ISO fs)
./scripts/iso/build-platform.sh     # Build ALL SOL services + sol-init daemons
./scripts/iso/assemble-rootfs.sh    # Assemble systemd-free root filesystem
./scripts/iso/create-iso.sh         # Generate ISO (initramfs + sol-boot + xorriso)
./scripts/iso/test-boot.sh          # Boot test in QEMU/OVMF

# Clean build artifacts
./scripts/iso/clean.sh
```

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│  Stage 1: Kernel Build                                  │
│  - Fetch latest stable from kernel.org                  │
│  - Apply SOL-specific config (build/kernel/sol.config)  │
│  - CONFIG_EFI_STUB / BLK_DEV_INITRD / ISO9660 / squashfs│
└────────────────┬────────────────────────────────────────┘
                 │
┌────────────────▼────────────────────────────────────────┐
│  Stage 2: Platform Build                                │
│  - cargo build --workspace --release                    │
│  - Install every SOL service binary                     │
│  - Install sol-init daemon definitions + D-Bus activation│
│  - No systemd units are generated                       │
└────────────────┬────────────────────────────────────────┘
                 │
┌────────────────▼────────────────────────────────────────┐
│  Stage 3: RootFS Assembly                               │
│  - Debian bookworm minbase WITHOUT systemd              │
│  - Merge kernel-staging + platform-staging              │
│  - /sbin/init -> sol-init (SOL bringup wrapper)         │
└────────────────┬────────────────────────────────────────┘
                 │
┌────────────────▼────────────────────────────────────────┐
│  Stage 4: ISO Creation                                  │
│  - busybox initramfs (no dracut)                        │
│  - sol-boot UEFI application + signed slot A (no GRUB)  │
│  - squashfs root + xorriso UEFI ISO                     │
└────────────────┬────────────────────────────────────────┘
                 │
┌────────────────▼────────────────────────────────────────┐
│  Stage 5: Boot Test                                     │
│  - Launch QEMU with OVMF (UEFI)                          │
│  - Wait for sol-init daemon startup markers             │
│  - Validate boot sequence                               │
└─────────────────────────────────────────────────────────┘
```

## Boot chain

```
EFI/BOOT/BOOTX64.EFI (sol-boot)
  └─ EFI/SOL/slots/A/system.efi   UKI: kernel + CONFIG_CMDLINE
      └─ kernel loads EFI/SOL/slots/A/initrd.img from the ESP
          └─ busybox initramfs mounts /live/filesystem.squashfs
              └─ overlay root -> switch_root -> /sbin/init
                  └─ sol-init supervises every SOL daemon
```

## Output

After successful build:
- **ISO**: `build/iso/sol-{version}-x86_64.iso` (UEFI-only)
- **Checksums**: `SHA256SUMS`, `MD5SUMS`

## CI/CD

GitHub Actions workflow (`.github/workflows/build-iso.yml`) builds on:
- Tags: `v*`
- Manual dispatch

Features:
- Caches kernel source and rootfs base
- Runs the QEMU/OVMF boot test
- Publishes to GitHub Releases

## Design constraints

- **sol-boot, not GRUB**: the ISO is a UEFI-only El Torito image; the EFI boot
  entry is sol-boot, which verifies and starts a signed slot deployment.
- **sol-init, not systemd**: the base rootfs is `--variant=minbase` with
  systemd excluded; sol-init supervises compositor, shell, and services via
  `.daemon` files with dependency ordering and restart policies.
- **No dracut**: the initramfs is a static busybox + a SOL-owned init script
  that mounts the live squashfs and hands control to `/sbin/init`.
- **All services ship**: settingsd, notificationd, portal, networkd, audiod,
  ntpd, diagnostics, deviced, init, plus the sol-files/terminal/settings apps.
- **Kernel cmdline**: sol-boot transfers control without a command line, so
  the initrd path is embedded via CONFIG_CMDLINE and the initrd file lives on
  the SOL ESP (`EFI/SOL/slots/A/initrd.img`).