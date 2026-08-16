#!/usr/bin/env bash
# Validate the Shell NetworkManager status provider against the live system bus.
set -euo pipefail

command -v busctl >/dev/null || {
  printf '%s\n' 'error: busctl is required for NetworkManager validation' >&2
  exit 1
}

busctl --system status org.freedesktop.NetworkManager >/dev/null
cargo test -p sol-shell --test networkmanager_dbus -- --ignored --exact networkmanager_status_round_trip
printf '%s\n' 'NetworkManager status validation passed.'
