#!/usr/bin/env bash
# Build the split packages from an isolated archive of the current Git revision.
# This intentionally does not install anything or read/write pacman configuration.
set -euo pipefail

readonly package_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly repository_dir="$(cd -- "${package_dir}/../.." && pwd)"
readonly pkgbase='sol'
readonly pkgver='0.1.0'
readonly pkgrel='1'
readonly arch='x86_64'
readonly source_dir="${pkgbase}-${pkgver}"
readonly expected_packages=(
  sol-compositor
  sol-session
  sol-shell
  sol-settingsd
  sol-notificationd
  sol-portal
  sol-ime
  sol-files
  sol-terminal
  sol-settings
  sol-desktop
)

for required_command in git makepkg bsdtar; do
  command -v "${required_command}" >/dev/null || {
    printf 'Missing required command: %s\n' "${required_command}" >&2
    exit 1
  }
done

test "$(id -u)" -ne 0 || {
  printf '%s\n' 'makepkg must run as an unprivileged user.' >&2
  exit 1
}

readonly work_dir="$(mktemp -d "${TMPDIR:-/tmp}/sol-arch-local-build.XXXXXX")"
trap 'rm -rf -- "${work_dir}"' EXIT
readonly source_archive="${work_dir}/${source_dir}.tar.gz"
readonly build_dir="${work_dir}/build"

git -C "${repository_dir}" archive --format=tar.gz --prefix="${source_dir}/" HEAD >"${source_archive}"
mkdir -- "${build_dir}"
cp "${package_dir}/PKGBUILD" "${build_dir}/PKGBUILD"
mv "${source_archive}" "${build_dir}/"

(
  cd "${build_dir}"
  makepkg --nodeps --cleanbuild
)

archive_members() {
  bsdtar -tf "${1}" | sed 's#^\./##'
}

require_member() {
  local archive="$1"
  local expected_member="$2"

  archive_members "${archive}" | grep -Fqx "${expected_member}" || {
    printf 'Missing %s in %s\n' "${expected_member}" "${archive}" >&2
    exit 1
  }
}

require_empty_meta_payload() {
  local archive="$1"
  local member

  while IFS= read -r member; do
    case "${member}" in
      .PKGINFO|.BUILDINFO|.MTREE) ;;
      *)
        printf 'Unexpected payload member %s in meta package %s\n' "${member}" "${archive}" >&2
        exit 1
        ;;
    esac
  done < <(archive_members "${archive}")
}

readonly archives_dir="${build_dir}"
mapfile -t actual_archives < <(
  find "${archives_dir}" -maxdepth 1 -type f -name "${pkgbase}-*.pkg.tar.*" -printf '%f\n' | sort
)

test "${#actual_archives[@]}" -eq "${#expected_packages[@]}" || {
  printf 'Expected %d package archives, found %d\n' \
    "${#expected_packages[@]}" "${#actual_archives[@]}" >&2
  exit 1
}

for package in "${expected_packages[@]}"; do
  mapfile -t matching_archives < <(
    find "${archives_dir}" -maxdepth 1 -type f \
      -name "${package}-${pkgver}-${pkgrel}-${arch}.pkg.tar.*" -print
  )
  test "${#matching_archives[@]}" -eq 1 || {
    printf 'Expected one archive for %s, found %d\n' "${package}" "${#matching_archives[@]}" >&2
    exit 1
  }

  archive="${matching_archives[0]}"
  if test "${package}" = 'sol-desktop'; then
    require_empty_meta_payload "${archive}"
  else
    require_member "${archive}" "usr/bin/${package}"
  fi

  if test "${package}" = 'sol-session'; then
    require_member "${archive}" 'usr/share/wayland-sessions/sol.desktop'
  fi
done

printf '%s\n' 'Local Arch split-package build validation passed.'
