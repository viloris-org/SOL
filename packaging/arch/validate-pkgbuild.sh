#!/usr/bin/env bash
# Static package metadata validation. It deliberately avoids release downloads.
set -euo pipefail

readonly package_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly pkgbuild="${package_dir}/PKGBUILD"
readonly expected_packages='sol-compositor sol-session sol-shell sol-settingsd sol-notificationd sol-portal sol-ime sol-files sol-terminal sol-settings sol-desktop'
readonly expected_session_dependencies='sol-compositor sol-notificationd sol-portal sol-settingsd sol-shell'
readonly expected_desktop_dependencies='sol-compositor sol-session sol-shell sol-settingsd sol-notificationd sol-portal sol-ime sol-files sol-terminal sol-settings'

bash -n "${pkgbuild}"

srcinfo="$(cd "${package_dir}" && makepkg --printsrcinfo)"
actual_packages="$(awk '$1 == "pkgname" { print $3 }' <<<"${srcinfo}" | paste -sd ' ' -)"
actual_session_dependencies="$(awk '
  $1 == "pkgname" && $3 == "sol-session" { in_session = 1; next }
  $1 == "pkgname" { in_session = 0 }
  in_session && $1 == "depends" { print $3 }
' <<<"${srcinfo}" | paste -sd ' ' -)"
actual_desktop_dependencies="$(awk '
  $1 == "pkgname" && $3 == "sol-desktop" { in_desktop = 1; next }
  $1 == "pkgname" { in_desktop = 0 }
  in_desktop && $1 == "depends" { print $3 }
' <<<"${srcinfo}" | paste -sd ' ' -)"

test "${actual_packages}" = "${expected_packages}"
test "${actual_session_dependencies}" = "${expected_session_dependencies}"
test "${actual_desktop_dependencies}" = "${expected_desktop_dependencies}"

printf '%s\n' 'PKGBUILD metadata validation passed.'
