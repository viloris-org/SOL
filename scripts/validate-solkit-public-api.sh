#!/usr/bin/env bash
# Validate the unpublished Public SolKit crate boundary.
set -euo pipefail

command -v cargo >/dev/null || {
  printf '%s\n' 'error: cargo is required for SolKit API validation' >&2
  exit 1
}
command -v jq >/dev/null || {
  printf '%s\n' 'error: jq is required for structured Cargo metadata checks' >&2
  exit 1
}

readonly expected_version='0.1.0'
readonly public_crates=(sol-app sol-design sol-ui sol-graphics sol-animation)
readonly metadata="$(cargo metadata --format-version 1 --no-deps --locked)"

for crate in "${public_crates[@]}"; do
  jq -e --arg crate "${crate}" --arg version "${expected_version}" '
    .packages[] | select(.name == $crate)
    | (.version == $version and .publish == []
       and any(.targets[]; (.kind | index("lib")) != null))
  ' <<<"${metadata}" >/dev/null || {
    printf 'Public crate metadata contract failed: %s\n' "${crate}" >&2
    exit 1
  }
done

# Public crates may depend on one another, but never on compositor, shell,
# session, or service implementation crates.
jq -e '
  [.packages[] | select(.name as $name |
    ["sol-app", "sol-design", "sol-ui", "sol-graphics", "sol-animation"]
    | index($name) != null)
   | .dependencies[].name
   | select(test("^(sol-compositor|sol-shell|sol-session|sol-settingsd|sol-notificationd|sol-portal|sol-ime|sol-diagnostics)$"))]
  | length == 0
' <<<"${metadata}" >/dev/null || {
  printf '%s\n' 'Public SolKit dependency direction contract failed.' >&2
  exit 1
}

printf '%s\n' 'Public SolKit API metadata validation passed.'
