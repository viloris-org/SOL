#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
MANIFEST_PATH="$ROOT_DIR/Cargo.toml"
FULL_CHECK=0

usage() {
    printf 'usage: %s [--full] [--manifest PATH]\n' "$(basename "$0")"
}

while (($# > 0)); do
    case "$1" in
        --full)
            FULL_CHECK=1
            shift
            ;;
        --manifest)
            if (($# < 2)); then
                usage >&2
                exit 2
            fi
            MANIFEST_PATH="$2"
            shift 2
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            usage >&2
            exit 2
            ;;
    esac
done

if [[ "$MANIFEST_PATH" != /* ]]; then
    MANIFEST_PATH="$PWD/$MANIFEST_PATH"
fi

failures=0
check() {
    local label="$1"
    shift
    if "$@" >/dev/null 2>&1; then
        printf '[ok]   %s\n' "$label"
    else
        printf '[fail] %s\n' "$label"
        failures=$((failures + 1))
    fi
}

printf 'SolKit doctor\n'
check 'cargo is available' command -v cargo
check 'rustc is available' command -v rustc
check 'manifest exists' test -f "$MANIFEST_PATH"

if [[ -f "$MANIFEST_PATH" ]]; then
    check 'Cargo metadata is valid' cargo metadata \
        --manifest-path "$MANIFEST_PATH" --locked --no-deps --format-version 1
fi

if [[ "$MANIFEST_PATH" == "$ROOT_DIR/Cargo.toml" ]]; then
    check 'starter copy-out validation passes' "$ROOT_DIR/scripts/validate-solkit-starter.sh"
fi

if ((FULL_CHECK)); then
    check 'locked workspace check passes' cargo check \
        --manifest-path "$MANIFEST_PATH" --locked --workspace
else
    printf '[skip] workspace check (pass --full to run it)\n'
fi

if ((failures > 0)); then
    printf 'SolKit doctor found %d problem(s).\n' "$failures" >&2
    exit 1
fi
printf 'SolKit doctor passed.\n'
