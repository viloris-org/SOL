# ADR-0030: ISO Build System Architecture

**Status:** Accepted (revised 2026-08-27 for sol-boot / sol-init boot path)  
**Date:** 2024-01-27  
**Context:** Phase 0 - Foundation

> **2026-08-27 revision:** this ADR originally standardized on GRUB, dracut and
> systemd units. Since then SOL shipped its own UEFI bootloader (`sol-boot`,
> ADR-0026) and daemon supervisor (`sol-init`), and dropped third-party runtime
> components from the platform (ADR-0028). The ISO build now uses sol-boot for
> the bootloader, a busybox initramfs instead of dracut, and sol-init instead
> of systemd units; the boot target is UEFI-only (no legacy BIOS/GRUB).

## Context

SOL needs a reproducible, automated ISO build pipeline for:
- Development testing and validation
- Release distribution
- CI/CD automation
- Developer onboarding

The ISO must be:
- Bootable on real hardware (UEFI)
- Testable in QEMU/VMs (OVMF)
- Built from the latest stable kernel
- Include all SOL platform components
- Reproducible across environments

## Decision

We implement a **staged build pipeline** with five distinct phases:

### 1. Kernel Build
- Fetch latest stable kernel from kernel.org automatically
- Apply SOL-specific configuration (`build/kernel/sol.config`)
- Build with optimizations for size and boot time
- Cache kernel source between builds

### 2. Platform Build
- Build all Rust workspace components in release mode
- Install binaries to staging rootfs
- Install SDK libraries
- Install sol-init daemon definitions (`.daemon`) and D-Bus activation files
- No systemd units are generated

### 3. RootFS Assembly
- Use Debian bookworm minbase WITHOUT systemd
- Merge kernel-staging and platform-staging into the rootfs
- Install the SOL `/sbin/init` bringup (hands control to sol-init)

### 4. ISO Creation
- Create squashfs from rootfs (xz compression)
- Generate the busybox initramfs (locates the live squashfs, overlays it)
- Provision the signed sol-boot ESP (UKI + manifest + deployment + state)
- Generate the UEFI-only ISO with xorriso (no GRUB)

### 5. Boot Testing
- Launch QEMU + OVMF with the ISO
- Validate sol-init startup (daemon supervision markers)

### Architecture Choices

**Kernel:** Latest stable from kernel.org, not distribution-provided
- Reasoning: Avoid distribution packaging lag, direct upstream access
- Tradeoff: Slightly longer build time vs. known-good kernel version

**Base System:** Debian bookworm minimal
- Reasoning: Stable, well-tested, minimal package set
- Alternative considered: Alpine (smaller but less tested with systemd)

**Compression:** SquashFS with xz
- Reasoning: Best compression ratio, read-only root safety
- Alternative: erofs (faster but less mature tooling)

**Bootloader:** sol-boot (UEFI, signed slots)
- Reasoning: SOL owns its boot policy (ADR-0026); GRUB cannot verify the
  SOL deployment envelope or drive A/B trials
- Alternative considered: GRUB 2 (rejected: third-party runtime component,
  BIOS legacy mode, no slot awareness)

**CI Platform:** GitHub Actions
- Reasoning: Native integration, generous free tier, good caching
- Containers: Debian bookworm for reproducibility

## Implementation

### Scripts
- `scripts/build-iso.sh` - Master orchestrator
- `scripts/iso/build-kernel.sh` - Kernel build stage
- `scripts/iso/build-platform.sh` - SOL components (all services + daemons)
- `scripts/iso/assemble-rootfs.sh` - Filesystem assembly (systemd-free)
- `scripts/iso/build-initramfs.sh` - busybox initramfs generation
- `scripts/iso/create-iso.sh` - ISO generation (sol-boot ESP + xorriso)
- `scripts/iso/test-boot.sh` - QEMU/OVMF validation
- `scripts/iso/clean.sh` - Artifact cleanup

### Configuration
- `build/kernel/sol.config` - Kernel configuration
- `build/rootfs/` - Custom overlay files
- `.github/workflows/build-iso.yml` - CI/CD pipeline

### Caching Strategy
1. **Kernel source**: Cached by version hash
2. **Rootfs base**: Cached by debootstrap script hash
3. **Rust artifacts**: Standard cargo/sccache

Build times:
- Clean build: ~45 minutes
- Cached build: ~15 minutes

## Consequences

### Positive
- Fully automated ISO generation from tag push
- Reproducible builds across environments
- Fast iteration with aggressive caching
- Easy developer onboarding (single script)
- CI/CD integration for releases
- Testable in QEMU before hardware deployment

### Negative
- Requires significant dependencies (kernel build tools, dosfstools/mtools,
  OVMF for the boot test, etc.)
- First build is slow (~45min)
- Requires root/sudo for debootstrap
- Large disk space requirement (~15GB for full build)
- UEFI-only: legacy BIOS machines are out of scope (sol-boot is UEFI)

### Neutral
- Uses Debian base (not a from-scratch rootfs)
  - Acceptable for Phase 0, may build custom base later
- The kernel carries CONFIG_CMDLINE with the initrd path
  - sol-boot transfers control without a loaded-image command line
- x86_64 only initially
  - aarch64 can be added later with matrix builds

## Future Considerations

### Phase 1+
- Custom minimal rootfs (replace Debian base)
- Image-based updates (OSTree/A-B partitions)
- Signed images and secure boot
- Multi-architecture (ARM64)
- Custom initramfs (replace dracut)

### Production Distribution
- CDN upload integration
- Delta/incremental updates
- Release signing workflow
- Torrent distribution

## References
- [docs/iso-build.md](../iso-build.md) - User documentation
- [build/README.md](../../build/README.md) - Detailed architecture
- `.github/workflows/build-iso.yml` - CI implementation
- [ADR-0028](ADR-0028-scp-only-no-wayland.md) - SCP-only compositor
