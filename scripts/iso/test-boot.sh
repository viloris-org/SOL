#!/bin/bash
# Boot-test the SOL ISO under QEMU + OVMF (UEFI).
#
# Success markers are SOL-owned (no systemd greps):
#   - "SOL Init starting"           sol-init reached userspace (PID 1 chain OK)
#   - "Starting daemon: sol-compositor"  the compositor daemon was supervised
#
# The ISO is UEFI-only (sol-boot), so the test requires OVMF firmware.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILD_DIR="${PROJECT_ROOT}/build"
ISO_OUTPUT="${BUILD_DIR}/iso"

if [ $# -lt 1 ]; then
    ISO_FILE=$(find "${ISO_OUTPUT}" -name "sol-*.iso" -type f -printf '%T@ %p\n' \
        | sort -rn | head -1 | cut -d' ' -f2-)
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

echo "==> Testing SOL ISO boot (UEFI/OVMF)..."
echo "  ISO: $(basename "$ISO_FILE")"
echo "  Size: $(du -h "$ISO_FILE" | cut -f1)"

# Check for QEMU and OVMF firmware
if ! command -v qemu-system-x86_64 &> /dev/null; then
    echo "ERROR: qemu-system-x86_64 not found. Install qemu-system-x86."
    exit 1
fi

OVMF_CODE=""
OVMF_VARS=""
for candidate in \
    "${OVMF_CODE:-}" \
    /usr/share/edk2/x64/OVMF_CODE.4m.fd \
    /usr/share/edk2/x64/OVMF_CODE.fd \
    /usr/share/OVMF/OVMF_CODE.4m.fd \
    /usr/share/OVMF/OVMF_CODE.fd; do
    if [ -n "$candidate" ] && [ -f "$candidate" ]; then
        OVMF_CODE="$candidate"
        break
    fi
done
for candidate in \
    "${OVMF_VARS:-}" \
    /usr/share/edk2/x64/OVMF_VARS.4m.fd \
    /usr/share/edk2/x64/OVMF_VARS.fd \
    /usr/share/OVMF/OVMF_VARS.4m.fd \
    /usr/share/OVMF/OVMF_VARS.fd; do
    if [ -n "$candidate" ] && [ -f "$candidate" ]; then
        OVMF_VARS="$candidate"
        break
    fi
done
if [ -z "$OVMF_CODE" ] || [ -z "$OVMF_VARS" ]; then
    echo "ERROR: OVMF firmware not found. Install ovmf (or edk2-ovmf)."
    exit 1
fi
echo "  OVMF: ${OVMF_CODE}"

MEMORY="${QEMU_MEMORY:-2G}"
CPUS="${QEMU_CPUS:-2}"
TIMEOUT="${BOOT_TIMEOUT:-120}"

echo "==> Starting QEMU test (timeout: ${TIMEOUT}s)..."
echo "  Memory: ${MEMORY}"
echo "  CPUs: ${CPUS}"
echo ""

LOG_FILE=$(mktemp /tmp/sol-boot-test.XXXXXX.log)
VARS_COPY=$(mktemp /tmp/sol-boot-test-vars.XXXXXX.fd)
cp "${OVMF_VARS}" "${VARS_COPY}"

timeout "$TIMEOUT" qemu-system-x86_64 \
    -machine q35,accel=tcg \
    -m "$MEMORY" \
    -smp "$CPUS" \
    -drive if=pflash,format=raw,readonly=on,file="$OVMF_CODE" \
    -drive if=pflash,format=raw,file="$VARS_COPY" \
    -cdrom "$ISO_FILE" \
    -nodefaults \
    -no-reboot \
    -display none \
    -monitor none \
    -serial file:"$LOG_FILE" \
    > /dev/null 2>&1 &
QEMU_PID=$!

echo "QEMU started (PID: $QEMU_PID)"
echo "Waiting for SOL init markers..."

BOOT_SUCCESS=false
START_TIME=$(date +%s)

while kill -0 "$QEMU_PID" 2>/dev/null; do
    ELAPSED=$(($(date +%s) - START_TIME))

    if [ "$ELAPSED" -ge "$TIMEOUT" ]; then
        echo "✗ Boot test timed out after ${TIMEOUT}s"
        kill "$QEMU_PID" 2>/dev/null || true
        break
    fi

    # SOL-owned success markers
    if grep -q "SOL Init starting" "$LOG_FILE" 2>/dev/null \
        || grep -q "Starting daemon: sol-compositor" "$LOG_FILE" 2>/dev/null; then
        BOOT_SUCCESS=true
        echo "✓ SOL init reached userspace in ${ELAPSED}s"
        kill "$QEMU_PID" 2>/dev/null || true
        break
    fi

    # Known boot failures
    if grep -q "Kernel panic" "$LOG_FILE" 2>/dev/null; then
        echo "✗ Kernel panic detected"
        break
    fi

    sleep 1
done

wait "$QEMU_PID" 2>/dev/null || true

echo ""
echo "==> Boot log summary:"
if [ -f "$LOG_FILE" ]; then
    grep -E "SOL:|sol-boot|SOL boot|handoff|sol_init|sol-init|Starting daemon|failed|panic" \
        "$LOG_FILE" | tail -25 || true
    echo ""
    echo "Full log: $LOG_FILE"
fi

if [ "$BOOT_SUCCESS" = true ]; then
    echo ""
    echo "✓ ISO boot test PASSED"
    rm -f "$LOG_FILE" "$VARS_COPY"
    exit 0
fi

echo ""
echo "✗ ISO boot test FAILED"
echo "Check the log file for details: $LOG_FILE"
rm -f "$VARS_COPY"
exit 1