#!/bin/bash
# Master build script for SOL ISO
# Orchestrates all build stages

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "╔═══════════════════════════════════════════════════════════════╗"
echo "║                    SOL OS ISO Builder                         ║"
echo "╚═══════════════════════════════════════════════════════════════╝"
echo ""

# Check for required tools
echo "==> Checking dependencies..."
MISSING_DEPS=()

for tool in curl jq cargo mksquashfs grub-mkrescue debootstrap; do
    if ! command -v $tool &> /dev/null; then
        MISSING_DEPS+=($tool)
    fi
done

if [ ${#MISSING_DEPS[@]} -gt 0 ]; then
    echo "ERROR: Missing required tools: ${MISSING_DEPS[*]}"
    echo ""
    echo "Install them with:"
    echo "  Ubuntu/Debian:"
    echo "    sudo apt-get install curl jq cargo squashfs-tools grub-pc-bin \\"
    echo "      grub-efi-amd64-bin xorriso debootstrap dracut"
    exit 1
fi

echo "  ✓ All dependencies found"
echo ""

# Stage 1: Build kernel
echo "╔═══════════════════════════════════════════════════════════════╗"
echo "║ Stage 1: Building Linux Kernel                                ║"
echo "╚═══════════════════════════════════════════════════════════════╝"
"${SCRIPT_DIR}/iso/build-kernel.sh"
echo ""

# Stage 2: Build SOL platform
echo "╔═══════════════════════════════════════════════════════════════╗"
echo "║ Stage 2: Building SOL Platform                                ║"
echo "╚═══════════════════════════════════════════════════════════════╝"
"${SCRIPT_DIR}/iso/build-platform.sh"
echo ""

# Stage 3: Assemble rootfs
echo "╔═══════════════════════════════════════════════════════════════╗"
echo "║ Stage 3: Assembling Root Filesystem                           ║"
echo "╚═══════════════════════════════════════════════════════════════╝"
"${SCRIPT_DIR}/iso/assemble-rootfs.sh"
echo ""

# Stage 4: Create ISO
echo "╔═══════════════════════════════════════════════════════════════╗"
echo "║ Stage 4: Creating ISO Image                                   ║"
echo "╚═══════════════════════════════════════════════════════════════╝"
"${SCRIPT_DIR}/iso/create-iso.sh"
echo ""

# Stage 5: Test (optional)
echo "╔═══════════════════════════════════════════════════════════════╗"
echo "║ Stage 5: Testing ISO Boot (Optional)                          ║"
echo "╚═══════════════════════════════════════════════════════════════╝"
if command -v qemu-system-x86_64 &> /dev/null; then
    read -p "Run QEMU boot test? (y/N) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        "${SCRIPT_DIR}/iso/test-boot.sh"
    else
        echo "Skipping boot test"
    fi
else
    echo "QEMU not found, skipping boot test"
fi

echo ""
echo "╔═══════════════════════════════════════════════════════════════╗"
echo "║                    Build Complete!                            ║"
echo "╚═══════════════════════════════════════════════════════════════╝"
echo ""
echo "ISO images are located in: ${PROJECT_ROOT}/build/iso/"
echo ""
ls -lh "${PROJECT_ROOT}/build/iso/"*.iso 2>/dev/null || true
