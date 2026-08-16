#!/usr/bin/env bash
# Compile representative toolkit clients and run them against SOL's compositor.
set -euo pipefail

readonly script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly repo_root="$(cd -- "${script_dir}/.." && pwd)"
readonly fixture_dir="${repo_root}/tests/fixtures/compatibility"
readonly scratch_dir="$(mktemp -d "${TMPDIR:-/tmp}/sol-wayland-compat.XXXXXX")"
readonly runtime_dir="${scratch_dir}/runtime"
readonly socket_name="wayland-sol-compat-$$"
readonly compositor_log="${scratch_dir}/compositor.log"
compositor_pid=

cleanup() {
    if [[ -n "${compositor_pid}" ]]; then
        kill "${compositor_pid}" 2>/dev/null || true
        wait "${compositor_pid}" 2>/dev/null || true
    fi
    rm -rf "${scratch_dir}"
}
trap cleanup EXIT HUP INT TERM

for command in cargo cc c++ pkg-config timeout; do
    command -v "${command}" >/dev/null 2>&1 || {
        printf 'error: required command is unavailable: %s\n' "${command}" >&2
        exit 1
    }
done

for package in gtk4 Qt6Widgets sdl2; do
    pkg-config --exists "${package}" || {
        printf 'error: required toolkit development package is unavailable: %s\n' "${package}" >&2
        exit 1
    }
done

cc -std=c11 -Wall -Wextra -Werror \
    "${fixture_dir}/gtk4.c" \
    -o "${scratch_dir}/gtk4-probe" \
    $(pkg-config --cflags --libs gtk4)
c++ -std=c++17 -Wall -Wextra -Werror -fPIC \
    "${fixture_dir}/qt6.cpp" \
    -o "${scratch_dir}/qt6-probe" \
    $(pkg-config --cflags --libs Qt6Widgets)
cc -std=c11 -Wall -Wextra -Werror \
    "${fixture_dir}/sdl2.c" \
    -o "${scratch_dir}/sdl2-probe" \
    $(pkg-config --cflags --libs sdl2)

cargo build --quiet --locked -p sol-compositor
mkdir "${runtime_dir}"
chmod 700 "${runtime_dir}"

XDG_RUNTIME_DIR="${runtime_dir}" \
SOL_WAYLAND_SOCKET="${socket_name}" \
    "${repo_root}/target/debug/sol-compositor" --headless \
    >"${compositor_log}" 2>&1 &
compositor_pid=$!

socket_path="${runtime_dir}/${socket_name}"
for _ in $(seq 1 200); do
    [[ -S "${socket_path}" ]] && break
    if ! kill -0 "${compositor_pid}" 2>/dev/null; then
        printf '%s\n' 'error: compositor exited before creating its socket' >&2
        sed -n '1,200p' "${compositor_log}" >&2
        exit 1
    fi
    sleep 0.05
done
[[ -S "${socket_path}" ]] || {
    printf '%s\n' 'error: compositor socket did not become ready' >&2
    sed -n '1,200p' "${compositor_log}" >&2
    exit 1
}

run_probe() {
    local name=$1
    local marker=$2
    shift 2
    local output="${scratch_dir}/${name}.log"
    if ! timeout 10s env \
        XDG_RUNTIME_DIR="${runtime_dir}" \
        WAYLAND_DISPLAY="${socket_name}" \
        "$@" >"${output}" 2>&1; then
        printf 'error: %s compatibility probe failed\n' "${name}" >&2
        sed -n '1,200p' "${output}" >&2
        sed -n '1,200p' "${compositor_log}" >&2
        exit 1
    fi
    grep -F "${marker}" "${output}" >/dev/null || {
        printf 'error: %s probe did not confirm its Wayland backend\n' "${name}" >&2
        sed -n '1,200p' "${output}" >&2
        exit 1
    }
    printf '%s compatibility probe passed.\n' "${name}"
}

run_probe gtk4 compat:gtk4:wayland env GDK_BACKEND=wayland "${scratch_dir}/gtk4-probe"
run_probe qt6 compat:qt6:wayland env QT_QPA_PLATFORM=wayland "${scratch_dir}/qt6-probe"
run_probe sdl2 compat:sdl2:wayland env SDL_VIDEODRIVER=wayland "${scratch_dir}/sdl2-probe"

kill -0 "${compositor_pid}"
printf '%s\n' 'Wayland toolkit compatibility validation passed.'
