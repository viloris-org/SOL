#!/usr/bin/env bash
# Run the SolUI accessibility bridge against a real isolated AT-SPI bus.
set -euo pipefail

command -v dbus-run-session >/dev/null 2>&1 || {
    printf '%s\n' 'error: dbus-run-session is required for AT-SPI validation' >&2
    exit 1
}
[[ -x /usr/lib/at-spi-bus-launcher ]] || {
    printf '%s\n' 'error: /usr/lib/at-spi-bus-launcher is required' >&2
    exit 1
}
[[ -x /usr/lib/at-spi2-registryd ]] || {
    printf '%s\n' 'error: /usr/lib/at-spi2-registryd is required' >&2
    exit 1
}

dbus-run-session -- bash -c '
    set -euo pipefail
    address=$(gdbus call --session \
        --dest org.a11y.Bus \
        --object-path /org/a11y/bus \
        --method org.a11y.Bus.GetAddress \
        | sed -E "s/^\('\''(.*)'\'',\)$/\1/")
    AT_SPI_BUS_ADDRESS="$address" /usr/lib/at-spi2-registryd &
    registry_pid=$!
    cleanup_registry() {
        kill "$registry_pid" 2>/dev/null || true
        wait "$registry_pid" 2>/dev/null || true
    }
    trap cleanup_registry EXIT HUP INT TERM
    for _ in $(seq 1 100); do
        busctl --address="$address" --no-pager --no-legend list \
            | awk '\''$1 == "org.a11y.atspi.Registry" { found = 1 } END { exit !found }'\'' \
            && break
        sleep 0.05
    done
    busctl --address="$address" --no-pager --no-legend list \
        | awk '\''$1 == "org.a11y.atspi.Registry" { found = 1 } END { exit !found }'\''
    export AT_SPI_BUS_ADDRESS="$address"
    cargo test --locked -p sol-ui --features atspi --test atspi_bridge -- --nocapture
'
