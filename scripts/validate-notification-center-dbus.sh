#!/usr/bin/env bash
# Exercise sol-shell's notification center through the real daemon proxy.
set -euo pipefail

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

if [[ ${SOL_NOTIFICATION_CENTER_DBUS_INNER:-} != 1 ]]; then
    dbus-run-session -- env \
        SOL_NOTIFICATION_CENTER_DBUS_INNER=1 \
        "$repo_root/scripts/validate-notification-center-dbus.sh"
    exit 0
fi

cd "$repo_root"
readonly service_name='org.sol.Notifications1'
readonly object_path='/org/sol/Notifications1'
readonly interface_name='org.sol.Notifications1'

cargo run --quiet -p sol-notificationd -- --dbus &
daemon_pid=$!
cleanup_daemon() {
    kill "$daemon_pid" 2>/dev/null || true
    wait "$daemon_pid" 2>/dev/null || true
}
trap cleanup_daemon EXIT HUP INT TERM

for _ in {1..100}; do
    if busctl --user call "$service_name" "$object_path" "$interface_name" Query ssb active '' false >/dev/null 2>&1; then
        break
    fi
    sleep 0.05
done

busctl --user call "$service_name" "$object_path" "$interface_name" Query ssb active '' false >/dev/null
cargo test --quiet -p sol-shell --test notification_center_dbus -- --ignored --test-threads=1

printf '%s\n' 'sol-shell Notification Center D-Bus validation passed.'
