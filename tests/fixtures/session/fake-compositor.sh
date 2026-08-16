#!/usr/bin/env sh
set -eu

[ "${1:-}" = "--tty-udev" ]
: > "$XDG_RUNTIME_DIR/$SOL_WAYLAND_SOCKET"
trap 'exit 0' TERM INT
while :; do
    sleep 1
done
