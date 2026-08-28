#!/usr/bin/env bash
#
# Guards the SCP-only boundary (ADR-0028).
#
# Removing Wayland once is easy; keeping it out is the hard part. A transitive
# dependency, a re-added manifest entry, or a resurrected `WAYLAND_DISPLAY`
# would each quietly restore a second client protocol, so CI checks all three
# on every run. Uses grep rather than rg: GitHub runners do not ship ripgrep.

set -euo pipefail

readonly project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${project_root}"

readonly legacy='wayland|smithay|wlroots|xwayland'

fail() {
    printf 'error: %s\n' "$1" >&2
    exit 1
}

# 1. Nothing legacy in the resolved dependency graph, including dev/build
#    edges and every optional feature.
graph_hits="$(
    cargo tree --workspace --all-features --edges all --prefix none \
        | grep -iE "(^|[-_])(${legacy})([-_ ]|\$)" \
        | sort -u || true
)"
if [[ -n "${graph_hits}" ]]; then
    printf '%s\n' "${graph_hits}" >&2
    fail 'legacy compositor dependency remains in the Cargo graph'
fi

# 2. Nothing legacy declared in a manifest, even if unused and unresolved.
manifest_hits="$(
    find . -name Cargo.toml -not -path './target/*' -not -path './.git/*' -print0 \
        | xargs -0 grep -nEi "^[[:space:]]*(${legacy})[^[:space:]]*[[:space:]]*=" \
        || true
)"
if [[ -n "${manifest_hits}" ]]; then
    printf '%s\n' "${manifest_hits}" >&2
    fail 'legacy compositor dependency remains in a Cargo manifest'
fi

# 3. No active session code advertising a legacy display socket.
socket_hits="$(
    grep -rnE 'WAYLAND_DISPLAY|SOL_WAYLAND_SOCKET' \
        compositor/src shell/src session/src session/assets \
        services/sol-logind/src services/sol-ime/src sdk/sol-ui/src \
        || true
)"
if [[ -n "${socket_hits}" ]]; then
    printf '%s\n' "${socket_hits}" >&2
    fail 'active session code still exposes a legacy display socket'
fi

# 4. Retired paths stay retired. These are the files whose return would mean
#    the compatibility layer came back by copy-paste rather than by dependency.
readonly retired_paths=(
    compositor/src/state.rs
    compositor/src/state_v2.rs
    compositor/src/render/smithay_backend.rs
    compositor/src/udev_runtime.rs
    compositor/examples/test-client.rs
    compositor/tests/sol_session.rs
    scripts/validate-wayland-compatibility.sh
)

for retired_path in "${retired_paths[@]}"; do
    if [[ -e "${retired_path}" ]]; then
        fail "retired compositor path returned: ${retired_path}"
    fi
done

printf '%s\n' 'SCP-only boundary validation passed.'
