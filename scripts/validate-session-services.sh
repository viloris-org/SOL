#!/usr/bin/env bash
# Prove sol-session owns the lifecycle of the real typed D-Bus services.
set -euo pipefail

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

if [[ ${SOL_SESSION_SERVICES_INNER:-} != 1 ]]; then
    runtime_dir=$(mktemp -d "${TMPDIR:-/tmp}/sol-session-services.XXXXXX")
    cleanup_runtime() {
        rm -rf "$runtime_dir"
    }
    trap cleanup_runtime EXIT HUP INT TERM
    dbus-run-session -- env \
        SOL_SESSION_SERVICES_INNER=1 \
        XDG_RUNTIME_DIR="$runtime_dir" \
        XDG_CONFIG_HOME="$runtime_dir/config" \
        "$repo_root/scripts/validate-session-services.sh"
    exit 0
fi

cd "$repo_root"
cargo build --quiet -p sol-session -p sol-settingsd -p sol-notificationd -p sol-portal
export SOL_COMPOSITOR_BIN="$repo_root/tests/fixtures/session/fake-compositor.sh"
export SOL_SHELL_BIN="$repo_root/tests/fixtures/session/fake-shell.sh"
export SOL_SETTINGSD_BIN="$repo_root/target/debug/sol-settingsd"
export SOL_NOTIFICATIOND_BIN="$repo_root/target/debug/sol-notificationd"
export SOL_PORTAL_BIN="$repo_root/target/debug/sol-portal"

cargo run --quiet -p sol-session -- --socket wayland-sol-services &
session_pid=$!
cleanup_session() {
    kill "$session_pid" 2>/dev/null || true
    wait "$session_pid" 2>/dev/null || true
}
trap cleanup_session EXIT HUP INT TERM

for _ in {1..100}; do
    [[ ! -e "$XDG_RUNTIME_DIR/shell-started" ]] || break
    sleep 0.05
done
[[ -e "$XDG_RUNTIME_DIR/shell-started" ]]

for service in org.sol.Settings1 org.sol.Notifications1 org.sol.Portal1; do
    for _ in {1..100}; do
        if busctl --user status "$service" >/dev/null 2>&1; then
            break
        fi
        sleep 0.05
    done
    busctl --user status "$service" >/dev/null
done

busctl --user call org.sol.Settings1 /org/sol/Settings1 org.sol.Settings1 Snapshot >/dev/null
busctl --user call org.sol.Notifications1 /org/sol/Notifications1 org.sol.Notifications1 Query ssb active '' false >/dev/null
portal=$(busctl --user call org.sol.Portal1 /org/sol/Portal1 org.sol.Portal1 Request sss org.sol.files screen-capture '')
[[ $portal == *'"denied"'* ]]

kill "$session_pid"
wait "$session_pid"
trap - EXIT HUP INT TERM

for service in org.sol.Settings1 org.sol.Notifications1 org.sol.Portal1; do
    if busctl --user status "$service" >/dev/null 2>&1; then
        printf 'session service still owns bus name after shutdown: %s\n' "$service" >&2
        exit 1
    fi
done

printf '%s\n' 'sol-session service lifecycle validation passed.'
