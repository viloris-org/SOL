# ADR-0030: ISO Build System Architecture

**Status:** Accepted  
**Date:** 2024-01-27  
**Context:** Phase 0 - Foundation

## Context

SOL needs a reproducible, automated ISO build pipeline for:
- Development testing and validation
- Release distribution
- CI/CD automation
- Developer onboarding

The ISO must be:
- Bootable on real hardware (BIOS and UEFI)
- Testable in QEMU/VMs
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
- Generate systemd service files
- Install SDK libraries

### 3. RootFS Assembly
- Use Debian bookworm minimal as base system
- Apply SOL overlay from `build/rootfs/`
- Configure systemd, users, and autologin
- Enable SOL services

### 4. ISO Creation
- Create squashfs from rootfs (xz compression)
- Generate initramfs with dracut (overlayfs support)
- Install GRUB with hybrid BIOS/UEFI
- Generate checksums

### 5. Boot Testing
- Launch QEMU with the ISO
- Validate compositor startup
- Check system boot markers

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

**Bootloader:** GRUB 2 with hybrid support
- Reasoning: Universal compatibility, BIOS + UEFI in one ISO
- Alternative: systemd-boot (UEFI-only, simpler but less compatible)

**CI Platform:** GitHub Actions
- Reasoning: Native integration, generous free tier, good caching
- Containers: Debian bookworm for reproducibility

## Implementation

### Scripts
- `scripts/build-iso.sh` - Master orchestrator
- `scripts/iso/build-kernel.sh` - Kernel build stage
- `scripts/iso/build-platform.sh` - SOL components
- `scripts/iso/assemble-rootfs.sh` - Filesystem assembly
- `scripts/iso/create-iso.sh` - ISO generation
- `scripts/iso/test-boot.sh` - QEMU validation
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
- Requires significant dependencies (kernel build tools, GRUB, etc.)
- First build is slow (~45min)
- Requires root/sudo for debootstrap
- Large disk space requirement (~15GB for full build)

### Neutral
- Uses Debian base (not a from-scratch rootfs)
  - Acceptable for Phase 0, may build custom base later
- GRUB instead of systemd-boot
  - Broader compatibility at cost of complexity
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
