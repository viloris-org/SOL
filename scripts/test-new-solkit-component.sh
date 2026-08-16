#!/usr/bin/env sh
# Exercise the component scaffolder without mutating the SOL checkout.
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/.." && pwd)
scaffold="$script_dir/new-solkit-component.sh"
scratch_dir=$(mktemp -d "${TMPDIR:-/tmp}/solkit-component-scaffold.XXXXXX")
project_dir="$scratch_dir/notes-controls"

cleanup() {
    rm -rf "$scratch_dir"
}
trap cleanup EXIT HUP INT TERM

"$scaffold" "$project_dir" "notes-controls"

grep -F 'name = "notes-controls"' "$project_dir/Cargo.toml"
grep -F 'name = "notes-controls"' "$project_dir/Cargo.lock"
grep -F "path = \"$repo_root/sdk/sol-design\"" "$project_dir/Cargo.toml"
grep -F "path = \"$repo_root/sdk/sol-ui\"" "$project_dir/Cargo.toml"
test ! -e "$project_dir/src/main.rs"

cargo test --manifest-path "$project_dir/Cargo.toml"

if "$scaffold" "$scratch_dir/bad-package" "Notes"; then
    printf '%s\n' "invalid package name unexpectedly succeeded" >&2
    exit 1
fi
[ ! -e "$scratch_dir/bad-package" ]

existing_dir="$scratch_dir/existing"
mkdir "$existing_dir"
touch "$existing_dir/keep"
if "$scaffold" "$existing_dir" "notes-controls"; then
    printf '%s\n' "existing destination unexpectedly succeeded" >&2
    exit 1
fi
[ -f "$existing_dir/keep" ]

if "$scaffold" "$repo_root/.solkit-component-forbidden" "notes-controls"; then
    printf '%s\n' "in-repository destination unexpectedly succeeded" >&2
    exit 1
fi
[ ! -e "$repo_root/.solkit-component-forbidden" ]

printf '%s\n' "new-solkit-component validation passed"
