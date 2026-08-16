#!/usr/bin/env bash
# Exercise the real portal authorization daemon through an isolated user bus.
set -euo pipefail

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

if [[ ${SOL_PORTAL_DBUS_INNER:-} != 1 ]]; then
    dbus-run-session -- env \
        SOL_PORTAL_DBUS_INNER=1 \
        "$repo_root/scripts/validate-portal-dbus.sh"
    exit 0
fi

cd "$repo_root"
readonly service_name='org.sol.Portal1'
readonly object_path='/org/sol/Portal1'
readonly interface_name='org.sol.Portal1'

cargo run --quiet -p sol-portal -- --dbus &
daemon_pid=$!
cleanup_daemon() {
    kill "$daemon_pid" 2>/dev/null || true
    wait "$daemon_pid" 2>/dev/null || true
}
trap cleanup_daemon EXIT HUP INT TERM

for _ in {1..100}; do
    if busctl --user call "$service_name" "$object_path" "$interface_name" Request sss org.sol.files screen-capture '' >/dev/null 2>&1; then
        break
    fi
    sleep 0.05
done

cargo test --quiet -p sol-portal --test dbus_proxy -- --ignored --test-threads=1

denied=$(busctl --user call "$service_name" "$object_path" "$interface_name" Request sss org.sol.files open-document file:///tmp/report.txt)
[[ $denied == *'"denied"'* ]]

if busctl --user call "$service_name" "$object_path" "$interface_name" Request sss org.sol.files arbitrary-kind '' >/dev/null 2>&1; then
    printf '%s\n' 'invalid portal request unexpectedly succeeded' >&2
    exit 1
fi

printf '%s\n' 'sol-portal D-Bus validation passed.'
