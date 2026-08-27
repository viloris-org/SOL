#!/usr/bin/env sh
set -eu

: > "$XDG_RUNTIME_DIR/$SOL_SCP_SOCKET"
trap 'exit 0' TERM INT
while :; do
    sleep 1
done
