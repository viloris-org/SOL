#!/usr/bin/env bash
# Validate Shell audio status and device inventory against live PipeWire.
set -euo pipefail

command -v pactl >/dev/null 2>&1 || {
    printf '%s\n' 'error: pactl is required for PipeWire audio validation' >&2
    exit 1
}

pactl --format=json info \
    | grep -F 'PipeWire' >/dev/null

cargo test --locked -p sol-shell --test pipewire_audio -- --ignored --nocapture
