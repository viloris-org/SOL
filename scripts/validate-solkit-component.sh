#!/usr/bin/env sh
# Validate the component template exactly as an external checkout would consume it.
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/.." && pwd)
scratch_dir=$(mktemp -d "${TMPDIR:-/tmp}/solkit-component.XXXXXX")
project_dir="$scratch_dir/solkit-component"

cleanup() {
    rm -rf "$scratch_dir"
}
trap cleanup EXIT HUP INT TERM

cp -R "$repo_root/templates/solkit-component" "$project_dir"
rm -rf "$project_dir/target"
sed -i "s|../../sdk|$repo_root/sdk|g" "$project_dir/Cargo.toml"

cargo test --manifest-path "$project_dir/Cargo.toml"
