#!/usr/bin/env bash
# Exercise the real typed notification daemon through an isolated user bus.
set -euo pipefail

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

if [[ ${SOL_NOTIFICATIOND_DBUS_INNER:-} != 1 ]]; then
    dbus-run-session -- env \
        SOL_NOTIFICATIOND_DBUS_INNER=1 \
        "$repo_root/scripts/validate-notificationd-dbus.sh"
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

cargo test --quiet -p sol-notificationd --test dbus_proxy -- --ignored --test-threads=1

history=$(busctl --user call "$service_name" "$object_path" "$interface_name" Query ssb history '' true)
[[ $history == *'"org.sol.files"'* ]]
[[ $history == *'"report-final.pdf"'* ]]
[[ $history == *'"dismissed-user"'* ]]

printf '%s\n' 'sol-notificationd D-Bus validation passed.'
