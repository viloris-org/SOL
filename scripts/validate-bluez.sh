#!/usr/bin/env bash
# Validate the Shell Bluetooth provider against the live BlueZ system service.
set -euo pipefail

command -v busctl >/dev/null 2>&1 || {
    printf '%s\n' 'error: busctl is required for BlueZ validation' >&2
    exit 1
}

busctl --system status org.bluez >/dev/null
cargo test --locked -p sol-shell --test bluez_dbus -- --ignored --exact bluez_status_round_trip
printf '%s\n' 'BlueZ status validation passed.'
