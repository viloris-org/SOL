#!/usr/bin/env bash
# Exercise the Shell Quick Settings model through a real isolated settingsd.
set -euo pipefail

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

if [[ ${SOL_QUICK_SETTINGS_DBUS_INNER:-} != 1 ]]; then
    scratch_dir=$(mktemp -d "${TMPDIR:-/tmp}/sol-quick-settings-dbus.XXXXXX")
    cleanup() {
        rm -rf "$scratch_dir"
    }
    trap cleanup EXIT HUP INT TERM
    dbus-run-session -- env \
        SOL_QUICK_SETTINGS_DBUS_INNER=1 \
        SOL_QUICK_SETTINGS_DBUS_SCRATCH="$scratch_dir" \
        "$repo_root/scripts/validate-quick-settings-dbus.sh"
    exit 0
fi

cd "$repo_root"

readonly settings_path="${SOL_QUICK_SETTINGS_DBUS_SCRATCH}/settings.conf"
readonly service_name='org.sol.Settings1'
readonly object_path='/org/sol/Settings1'
readonly interface_name='org.sol.Settings1'

SOL_SETTINGS_PATH="$settings_path" cargo run --quiet -p sol-settingsd -- --dbus &
daemon_pid=$!

cleanup_daemon() {
    kill "$daemon_pid" 2>/dev/null || true
    wait "$daemon_pid" 2>/dev/null || true
}
trap cleanup_daemon EXIT HUP INT TERM

for _ in {1..100}; do
    if busctl --user call "$service_name" "$object_path" "$interface_name" Snapshot >/dev/null 2>&1; then
        break
    fi
    sleep 0.05
done

busctl --user call "$service_name" "$object_path" "$interface_name" Snapshot >/dev/null
cargo test --quiet -p sol-shell --test quick_settings_dbus -- --ignored --test-threads=1

snapshot=$(busctl --user call "$service_name" "$object_path" "$interface_name" Snapshot)
[[ $snapshot == 'tsbbsyb 3 '* ]]
[[ $snapshot == *'"dark"'* ]]
[[ $snapshot == *' 67 true' ]]

printf '%s\n' 'Shell Quick Settings D-Bus validation passed.'
