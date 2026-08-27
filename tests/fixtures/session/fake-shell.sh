#!/usr/bin/env sh
set -eu

[ "$SOL_SCP_SOCKET" = "sol-compositor-services" ]
[ "$XDG_CURRENT_DESKTOP" = "SOL" ]
[ "$XDG_SESSION_DESKTOP" = "SOL" ]
: > "$XDG_RUNTIME_DIR/shell-started"
trap 'exit 0' TERM INT
while :; do
    sleep 1
done
