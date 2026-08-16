#!/usr/bin/env bash
# Validate the Shell power provider against the host UPower system service.
set -euo pipefail

command -v busctl >/dev/null 2>&1 || {
    printf '%s\n' 'error: busctl is required for UPower validation' >&2
    exit 1
}

busctl --system --no-pager --no-legend list \
    | awk '$1 == "org.freedesktop.UPower" { found = 1 } END { exit !found }'

cargo test --locked -p sol-shell --test upower_dbus -- --ignored --nocapture
