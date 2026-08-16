#!/usr/bin/env bash
# Exercise the real settings daemon through an isolated user session bus.
set -euo pipefail

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

if [[ ${SOL_SETTINGSD_DBUS_INNER:-} != 1 ]]; then
    scratch_dir=$(mktemp -d "${TMPDIR:-/tmp}/sol-settingsd-dbus.XXXXXX")
    cleanup() {
        rm -rf "$scratch_dir"
    }
    trap cleanup EXIT HUP INT TERM
    dbus-run-session -- env \
        SOL_SETTINGSD_DBUS_INNER=1 \
        SOL_SETTINGSD_DBUS_SCRATCH="$scratch_dir" \
        SOL_SETTINGSD_DBUS_TRACE="${SOL_SETTINGSD_DBUS_TRACE:-}" \
        "$repo_root/scripts/validate-settingsd-dbus.sh"
    exit 0
fi

cd "$repo_root"

readonly settings_path="${SOL_SETTINGSD_DBUS_SCRATCH}/settings.conf"
readonly service_name='org.sol.Settings1'
readonly object_path='/org/sol/Settings1'
readonly interface_name='org.sol.Settings1'

if [[ ${SOL_SETTINGSD_DBUS_TRACE:-} == 1 ]]; then
    set -x
fi

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

initial=$(busctl --user call "$service_name" "$object_path" "$interface_name" Snapshot)
[[ $initial == *'"system"'* ]]
[[ $initial == *' 50 false' ]]

cargo test --quiet -p sol-settingsd --test dbus_proxy -- --ignored --test-threads=1

busctl --user call "$service_name" "$object_path" "$interface_name" SetColorScheme s dark >/dev/null
busctl --user call "$service_name" "$object_path" "$interface_name" SetTextScale s large >/dev/null
busctl --user call "$service_name" "$object_path" "$interface_name" SetOutputVolume y 73 >/dev/null
busctl --user call "$service_name" "$object_path" "$interface_name" SetOutputMuted b true >/dev/null

updated=$(busctl --user call "$service_name" "$object_path" "$interface_name" Snapshot)
[[ $updated == 'tsbbsyb 5 '* ]]
[[ $updated == *'"dark"'* ]]
[[ $updated == *'"large"'* ]]
[[ $updated == *' 73 true' ]]

printf '%s\n' 'sol-settingsd D-Bus validation passed.'
