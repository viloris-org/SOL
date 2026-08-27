# SOL ISO Build System

This directory contains the complete CI/CD infrastructure for building bootable SOL OS ISO images.

## Architecture

The ISO build pipeline is split into 5 distinct stages:

```
1. Kernel Build    → Latest stable Linux kernel with SOL-specific config
2. Platform Build  → SOL compositor, shell, services, and SDK
3. RootFS Assembly → Debian minimal base + SOL components
4. ISO Creation    → Squashfs + GRUB bootloader + hybrid ISO
5. Boot Testing    → QEMU smoke test validation
```

## Quick Start

### Local Build

```bash
# Build complete ISO (all stages)
./scripts/build-iso.sh

# Or run stages individually
./scripts/iso/build-kernel.sh      # Stage 1: Kernel
./scripts/iso/build-platform.sh    # Stage 2: SOL components
./scripts/iso/assemble-rootfs.sh   # Stage 3: Filesystem
./scripts/iso/create-iso.sh        # Stage 4: ISO image
./scripts/iso/test-boot.sh         # Stage 5: Boot test
```

### CI/CD

The GitHub Actions workflow `.github/workflows/build-iso.yml` automatically builds ISOs on:
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
│   ├── sol.config              # Kernel configuration
│   ├── linux-*.tar.xz          # Cached kernel source
│   └── linux-*/                # Extracted kernel build
├── rootfs/                     # Custom overlay files (optional)
│   ├── etc/
│   ├── usr/
│   └── lib/systemd/system/
├── rootfs-staging/             # Assembled root filesystem
├── iso-staging/                # ISO build staging area
└── iso/
    ├── sol-*.iso               # Final ISO image
    ├── SHA256SUMS              # Checksums
    └── MD5SUMS

scripts/
├── build-iso.sh                # Master build orchestrator
└── iso/
    ├── build-kernel.sh         # Stage 1: Kernel build
    ├── build-platform.sh       # Stage 2: SOL platform build
    ├── assemble-rootfs.sh      # Stage 3: RootFS assembly
    ├── create-iso.sh           # Stage 4: ISO generation
    └── test-boot.sh            # Stage 5: QEMU boot test
```

## Dependencies

### Ubuntu/Debian

```bash
sudo apt-get install \
  curl jq git rsync bc kmod cpio flex bison \
  build-essential pkg-config libssl-dev libelf-dev \
  debootstrap squashfs-tools xorriso grub-pc-bin \
  grub-efi-amd64-bin mtools dosfstools dracut \
  qemu-system-x86
```

### Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## Configuration

### Kernel Config

Edit `build/kernel/sol.config` to customize kernel features. The default config includes:
- DRM/KMS graphics (Intel, AMD, NVIDIA nouveau, virtio)
- Input devices (keyboard, mouse, touchpad)
- USB support
- SquashFS and OverlayFS
- Minimal networking (no wireless)

### RootFS Overlay

Add custom files to `build/rootfs/` to include them in the ISO:

```bash
mkdir -p build/rootfs/etc/sol
echo "custom_setting=value" > build/rootfs/etc/sol/config.conf
```

### Systemd Services

Service files are auto-generated in `scripts/iso/build-platform.sh`:
- `sol-compositor.service` - Main compositor
- `sol-shell.service` - Desktop shell
- `sol-settingsd.service` - Settings daemon
- `sol-notificationd.service` - Notifications

## Output

After a successful build:

```
build/iso/
├── sol-v0.1.0-x86_64.iso       # Bootable ISO image
├── SHA256SUMS                   # SHA-256 checksums
└── MD5SUMS                      # MD5 checksums
```

## Testing

### QEMU (Virtual Machine)

```bash
# Quick test (2GB RAM, 2 cores)
qemu-system-x86_64 -m 2G -smp 2 -cdrom build/iso/sol-*.iso

# With virtio acceleration
qemu-system-x86_64 \
  -m 2G -smp 2 \
  -cdrom build/iso/sol-*.iso \
  -device virtio-vga \
  -netdev user,id=net0 \
  -device virtio-net-pci,netdev=net0
```

### USB Boot

```bash
# Write ISO to USB drive (replace /dev/sdX with your USB device)
sudo dd if=build/iso/sol-*.iso of=/dev/sdX bs=4M status=progress
sync
```

### Verification

```bash
cd build/iso
sha256sum -c SHA256SUMS
```

## CI/CD Features

### Caching

The GitHub Actions workflow caches:
- Rust build artifacts (`target/`)
- Kernel source tarballs
- Base rootfs (Debian bookworm)

This reduces build times from ~45min to ~15min on subsequent runs.

### Multi-Architecture (Future)

The current setup is x86_64-only. To add aarch64:

1. Add matrix strategy in `.github/workflows/build-iso.yml`
2. Update kernel config for ARM64
3. Adjust GRUB config for ARM UEFI

### Artifact Storage

ISOs are:
- Uploaded as GitHub Actions artifacts (30 day retention)
- Attached to GitHub Releases (permanent, for tagged builds)

To upload to a CDN, add your logic at the end of the workflow.

## Boot Process

1. **GRUB** loads kernel and initramfs
2. **Initramfs** (dracut) mounts squashfs and sets up overlayfs
3. **Systemd** starts as PID 1
4. **sol-compositor.service** starts the compositor
5. **sol-shell.service** starts the desktop shell
6. **Autologin** to `sol` user (no password)

## Troubleshooting

### Build fails at kernel stage

- Check kernel version: `curl -s https://www.kernel.org/releases.json | jq -r '.latest_stable.version'`
- Ensure you have enough disk space (kernel build needs ~10GB)

### Build fails at rootfs stage

- Ensure you have root/sudo access (debootstrap requires it)
- Check network connectivity (downloads Debian packages)

### ISO won't boot

- Verify GRUB config in `build/iso-staging/boot/grub/grub.cfg`
- Check initramfs was generated: `ls -lh build/iso-staging/boot/initrd.img`
- Test with verbose boot: Select "Debug" option in GRUB menu

### Compositor doesn't start

- Check logs: `journalctl -u sol-compositor`
- Verify binary was installed: `ls -l build/rootfs-staging/usr/bin/sol-compositor`
- Test in QEMU with serial console for debugging

## Performance

Typical build times (on GitHub Actions `ubuntu-latest`):

| Stage | Time (cached) | Time (clean) |
|-------|---------------|--------------|
| Kernel | 5-10 min | 20-30 min |
| Platform | 3-5 min | 10-15 min |
| RootFS | 2-3 min | 8-12 min |
| ISO | 1-2 min | 1-2 min |
| **Total** | **~15 min** | **~45 min** |

## Security

### Signing ISOs

To GPG-sign releases:

1. Add GPG key to GitHub Secrets (`GPG_PRIVATE_KEY`, `GPG_PASSPHRASE`)
2. Import in workflow:
   ```yaml
   - name: Import GPG key
     run: |
       echo "${{ secrets.GPG_PRIVATE_KEY }}" | gpg --import
       echo "${{ secrets.GPG_PASSPHRASE }}" | gpg --batch --yes --passphrase-fd 0 --detach-sign --armor SHA256SUMS
   ```

### Reproducible Builds

To ensure reproducible builds:
- Use fixed Debian version (bookworm)
- Pin kernel version or use tags
- Use `SOURCE_DATE_EPOCH` for timestamps

## Contributing

When modifying the build system:
1. Test locally with `./scripts/build-iso.sh`
2. Test in CI with workflow dispatch
3. Verify the ISO boots in QEMU
4. Update this README if adding new features

## License

Same as the SOL project root.
