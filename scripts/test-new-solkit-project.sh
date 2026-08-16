#!/usr/bin/env sh
# Exercise the scaffolder's success and rejection paths without mutating SOL.
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/.." && pwd)
scaffold="$script_dir/new-solkit-project.sh"
scratch_dir=$(mktemp -d "${TMPDIR:-/tmp}/solkit-scaffold-test.XXXXXX")
project_dir="$scratch_dir/notes-app"

cleanup() {
    rm -rf "$scratch_dir"
}
trap cleanup EXIT HUP INT TERM

"$scaffold" "$project_dir" "notes-app" "com.example.notes"

grep -F 'name = "notes-app"' "$project_dir/Cargo.toml"
grep -F 'name = "notes-app"' "$project_dir/Cargo.lock"
grep -F "path = \"$repo_root/sdk/sol-app\"" "$project_dir/Cargo.toml"
grep -F 'pub const APP_ID: &str = "com.example.notes";' "$project_dir/src/lib.rs"
grep -F 'let report = notes_app::run()?;' "$project_dir/src/main.rs"

cargo test --manifest-path "$project_dir/Cargo.toml"

if "$scaffold" "$scratch_dir/bad-package" "Notes" "com.example.notes"; then
    printf '%s\n' "invalid package name unexpectedly succeeded" >&2
    exit 1
fi
[ ! -e "$scratch_dir/bad-package" ]

if "$scaffold" "$scratch_dir/bad-app-id" "notes-app" "com..example"; then
    printf '%s\n' "invalid application ID unexpectedly succeeded" >&2
    exit 1
fi
[ ! -e "$scratch_dir/bad-app-id" ]

existing_dir="$scratch_dir/existing"
mkdir "$existing_dir"
touch "$existing_dir/keep"
if "$scaffold" "$existing_dir" "notes-app" "com.example.notes"; then
    printf '%s\n' "existing destination unexpectedly succeeded" >&2
    exit 1
fi
[ -f "$existing_dir/keep" ]

if "$scaffold" "$repo_root/.solkit-scaffold-forbidden" "notes-app" "com.example.notes"; then
    printf '%s\n' "in-repository destination unexpectedly succeeded" >&2
    exit 1
fi
[ ! -e "$repo_root/.solkit-scaffold-forbidden" ]

printf '%s\n' "new-solkit-project validation passed"
