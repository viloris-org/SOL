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
readonly freedesktop_service_name='org.freedesktop.Notifications'
readonly freedesktop_object_path='/org/freedesktop/Notifications'
readonly freedesktop_interface_name='org.freedesktop.Notifications'

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

capabilities=$(busctl --user call "$freedesktop_service_name" "$freedesktop_object_path" "$freedesktop_interface_name" GetCapabilities)
[[ $capabilities == *'"actions"'* ]]
server=$(busctl --user call "$freedesktop_service_name" "$freedesktop_object_path" "$freedesktop_interface_name" GetServerInformation)
[[ $server == *'"SOL Notification Daemon"'* ]]

signal_log=$(mktemp)
busctl --user monitor "$freedesktop_service_name" >"$signal_log" 2>&1 &
monitor_pid=$!
cleanup_monitor() {
    kill "$monitor_pid" 2>/dev/null || true
    wait "$monitor_pid" 2>/dev/null || true
    rm -f "$signal_log"
}
trap 'cleanup_monitor; cleanup_daemon' EXIT HUP INT TERM
sleep 0.05

# app_name is a validated reverse-DNS identity; no hints are needed here.
notification_id=$(busctl --user call "$freedesktop_service_name" "$freedesktop_object_path" "$freedesktop_interface_name" Notify susssasa{sv}i \
    org.sol.files 0 '' 'Interoperability test' 'body text' 2 open Open 0 0 | awk '{ print $2 }')
[[ $notification_id =~ ^[0-9]+$ ]]

# Replacing preserves the ID through the daemon's owner-checked replacement path.
replacement_id=$(busctl --user call "$freedesktop_service_name" "$freedesktop_object_path" "$freedesktop_interface_name" Notify susssasa{sv}i \
    org.sol.files "$notification_id" '' 'Interoperability replacement' 'body text' 2 open Open 0 0 | awk '{ print $2 }')
[[ $replacement_id == "$notification_id" ]]

# The typed action path emits the standard ActionInvoked signal; CloseNotification
# emits NotificationClosed with the standard application-close reason.
busctl --user call "$service_name" "$object_path" "$interface_name" InvokeAction ts "$notification_id" open >/dev/null
busctl --user call "$freedesktop_service_name" "$freedesktop_object_path" "$freedesktop_interface_name" CloseNotification u "$notification_id" >/dev/null
sleep 0.1
grep -q 'ActionInvoked' "$signal_log"
grep -q 'NotificationClosed' "$signal_log"

printf '%s\n' 'sol-notificationd D-Bus validation passed.'
