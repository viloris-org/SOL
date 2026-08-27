# SOL ISO Build System

Complete CI/CD pipeline for building bootable SOL OS ISO images with the latest stable kernel.

## Quick Start

```bash
# Build everything
./scripts/build-iso.sh

# Or run individual stages
./scripts/iso/build-kernel.sh       # Build kernel
./scripts/iso/build-platform.sh     # Build SOL components
./scripts/iso/assemble-rootfs.sh    # Assemble filesystem
./scripts/iso/create-iso.sh         # Generate ISO
./scripts/iso/test-boot.sh          # Test in QEMU

# Clean build artifacts
./scripts/iso/clean.sh
```

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│  Stage 1: Kernel Build                                  │
│  - Fetch latest stable from kernel.org                  │
│  - Apply SOL-specific config                            │
│  - Build with all cores                                 │
└────────────────┬────────────────────────────────────────┘
                 │
┌────────────────▼────────────────────────────────────────┐
│  Stage 2: Platform Build                                │
│  - cargo build --workspace --release                    │
│  - Install binaries and libraries                       │
│  - Generate systemd service files                       │
└────────────────┬────────────────────────────────────────┘
                 │
┌────────────────▼────────────────────────────────────────┐
│  Stage 3: RootFS Assembly                               │
│  - Debian bookworm minimal base                         │
│  - Apply SOL overlay                                    │
│  - Configure systemd, users, autologin                  │
└────────────────┬────────────────────────────────────────┘
                 │
┌────────────────▼────────────────────────────────────────┐
│  Stage 4: ISO Creation                                  │
│  - Create squashfs from rootfs                          │
│  - Generate initramfs with dracut                       │
│  - Install GRUB (BIOS + UEFI)                           │
│  - Generate hybrid ISO with xorriso                     │
└────────────────┬────────────────────────────────────────┘
                 │
┌────────────────▼────────────────────────────────────────┐
│  Stage 5: Boot Test                                     │
│  - Launch QEMU with ISO                                 │
│  - Wait for compositor start                            │
│  - Validate boot sequence                               │
└─────────────────────────────────────────────────────────┘
```

## Output

After successful build:
- **ISO**: `build/iso/sol-{version}-x86_64.iso`
- **Checksums**: `SHA256SUMS`, `MD5SUMS`
- **Bootable**: Hybrid BIOS/UEFI, USB-writable

## Testing

```bash
# QEMU
qemu-system-x86_64 -m 2G -smp 2 -cdrom build/iso/sol-*.iso

# USB (replace /dev/sdX)
sudo dd if=build/iso/sol-*.iso of=/dev/sdX bs=4M status=progress
```

## CI/CD

GitHub Actions workflow (`.github/workflows/build-iso.yml`) builds on:
- Tags: `v*`
- Manual dispatch

Features:
- Caches kernel source and rootfs base
- Runs QEMU boot test
- Publishes to GitHub Releases
- ~15min cached, ~45min clean

See [build/README.md](build/README.md) for detailed documentation.
