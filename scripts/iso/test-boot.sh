#!/bin/bash
# Test SOL ISO boot in QEMU
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILD_DIR="${PROJECT_ROOT}/build"
ISO_OUTPUT="${BUILD_DIR}/iso"

if [ $# -lt 1 ]; then
    # Find the latest ISO
    ISO_FILE=$(find "${ISO_OUTPUT}" -name "sol-*.iso" -type f -printf '%T@ %p\n' | sort -rn | head -1 | cut -d' ' -f2-)
    if [ -z "$ISO_FILE" ]; then
        echo "ERROR: No ISO file found in ${ISO_OUTPUT}"
        exit 1
    fi
else
    ISO_FILE="$1"
fi

if [ ! -f "$ISO_FILE" ]; then
    echo "ERROR: ISO file not found: $ISO_FILE"
    exit 1
fi

echo "==> Testing SOL ISO boot..."
echo "  ISO: $(basename "$ISO_FILE")"
echo "  Size: $(du -h "$ISO_FILE" | cut -f1)"
echo ""

# Check for QEMU
if ! command -v qemu-system-x86_64 &> /dev/null; then
    echo "ERROR: qemu-system-x86_64 not found. Install it first:"
    echo "  Ubuntu/Debian: sudo apt-get install qemu-system-x86"
    exit 1
fi

# Test parameters
MEMORY="${QEMU_MEMORY:-2G}"
CPUS="${QEMU_CPUS:-2}"
TIMEOUT="${BOOT_TIMEOUT:-60}"

echo "==> Starting QEMU test (timeout: ${TIMEOUT}s)..."
echo "  Memory: ${MEMORY}"
echo "  CPUs: ${CPUS}"
echo ""

# Create temporary log file
LOG_FILE=$(mktemp /tmp/sol-boot-test.XXXXXX.log)

# Run QEMU in background with serial console logging
timeout "$TIMEOUT" qemu-system-x86_64 \
    -m "$MEMORY" \
    -smp "$CPUS" \
    -cdrom "$ISO_FILE" \
    -device virtio-vga \
    -netdev user,id=net0 \
    -device virtio-net-pci,netdev=net0 \
    -serial file:"$LOG_FILE" \
    -display none \
    -no-reboot \
    > /dev/null 2>&1 &

QEMU_PID=$!

echo "QEMU started (PID: $QEMU_PID)"
echo "Waiting for boot markers..."

# Wait for boot success markers
BOOT_SUCCESS=false
START_TIME=$(date +%s)

while kill -0 $QEMU_PID 2>/dev/null; do
    ELAPSED=$(($(date +%s) - START_TIME))
    
    if [ $ELAPSED -ge $TIMEOUT ]; then
        echo "✗ Boot test timed out after ${TIMEOUT}s"
        kill $QEMU_PID 2>/dev/null || true
        break
    fi
    
    # Check for success markers in log
    if grep -q "sol-compositor.*started" "$LOG_FILE" 2>/dev/null || \
       grep -q "Reached target.*Graphical Interface" "$LOG_FILE" 2>/dev/null; then
        BOOT_SUCCESS=true
        echo "✓ Boot test passed in ${ELAPSED}s"
        kill $QEMU_PID 2>/dev/null || true
        break
    fi
    
    # Check for boot failures
    if grep -q "Kernel panic" "$LOG_FILE" 2>/dev/null; then
        echo "✗ Kernel panic detected"
        break
    fi
    
    sleep 1
done

# Wait for QEMU to exit
wait $QEMU_PID 2>/dev/null || true

echo ""
echo "==> Boot log summary:"
if [ -f "$LOG_FILE" ]; then
    # Show relevant log lines
    grep -E "(sol-|systemd\[1\]|kernel:|Starting|Reached target)" "$LOG_FILE" | tail -20 || true
    echo ""
    echo "Full log: $LOG_FILE"
fi

if [ "$BOOT_SUCCESS" = true ]; then
    echo ""
    echo "✓ ISO boot test PASSED"
    rm -f "$LOG_FILE"
    exit 0
else
    echo ""
    echo "✗ ISO boot test FAILED"
    echo "Check the log file for details: $LOG_FILE"
    exit 1
fi
