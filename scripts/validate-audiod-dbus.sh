#!/usr/bin/env bash
# Exercise sol-audiod through an isolated session bus and deterministic pactl.
set -euo pipefail

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

if [[ ${SOL_AUDIOD_DBUS_INNER:-} != 1 ]]; then
    scratch_dir=$(mktemp -d "${TMPDIR:-/tmp}/sol-audiod-dbus.XXXXXX")
    cleanup() {
        rm -rf "$scratch_dir"
    }
    trap cleanup EXIT HUP INT TERM
    dbus-run-session -- env \
        SOL_AUDIOD_DBUS_INNER=1 \
        SOL_AUDIOD_DBUS_SCRATCH="$scratch_dir" \
        "$repo_root/scripts/validate-audiod-dbus.sh"
    exit 0
fi

cd "$repo_root"
readonly service_name='org.sol.Audio1'
readonly object_path='/org/sol/Audio1'
readonly interface_name='org.sol.Audio1'
readonly fake_bin="${SOL_AUDIOD_DBUS_SCRATCH}/bin"
readonly default_sink="${SOL_AUDIOD_DBUS_SCRATCH}/default-sink"

mkdir -p "$fake_bin" "${SOL_AUDIOD_DBUS_SCRATCH}/config"
printf '%s\n' 'alsa_output.speaker' >"$default_sink"
cat >"$fake_bin/pactl" <<'FAKE_PACTL'
#!/usr/bin/env bash
set -euo pipefail

if [[ $* == '--format=json info' ]]; then
    printf '{"server_name":"PulseAudio (on PipeWire 1.6.8)","default_sink_name":"%s"}\n' \
        "$(<"${SOL_AUDIOD_DBUS_SCRATCH}/default-sink")"
elif [[ $* == '--format=json list sinks' ]]; then
    cat <<'JSON'
[
  {
    "name":"alsa_output.speaker",
    "description":"SOL Built-in Speakers",
    "driver":"PipeWire",
    "properties":{"device.bus":"pci","device.form_factor":"speaker"},
    "active_port":"analog-output-speaker"
  },
  {
    "name":"bluez_output.00_11_22_33_44_55.1",
    "description":"Sony WH-1000XM5",
    "driver":"PipeWire",
    "properties":{"device.bus":"bluetooth"},
    "active_port":null
  }
]
JSON
elif [[ $* == '--format=json list sink-inputs' ]]; then
    printf '[]\n'
elif [[ ${1:-} == 'set-default-sink' ]]; then
    printf '%s\n' "$2" >"${SOL_AUDIOD_DBUS_SCRATCH}/default-sink"
elif [[ ${1:-} == 'move-sink-input' ]]; then
    :
else
    printf 'unsupported fake pactl invocation: %s\n' "$*" >&2
    exit 2
fi
FAKE_PACTL
chmod +x "$fake_bin/pactl"

PATH="$fake_bin:$PATH" \
XDG_CONFIG_HOME="${SOL_AUDIOD_DBUS_SCRATCH}/config" \
    cargo run --quiet -p sol-audiod &
daemon_pid=$!
cleanup_daemon() {
    kill "$daemon_pid" 2>/dev/null || true
    wait "$daemon_pid" 2>/dev/null || true
}
trap cleanup_daemon EXIT HUP INT TERM

for _ in {1..100}; do
    if busctl --user call "$service_name" "$object_path" "$interface_name" ListDevices >/dev/null 2>&1; then
        break
    fi
    sleep 0.05
done

devices=$(busctl --user call "$service_name" "$object_path" "$interface_name" ListDevices)
[[ $devices == *'alsa_output.speaker'* ]]
[[ $devices == *'bluez_output.00_11_22_33_44_55.1'* ]]

busctl --user call "$service_name" "$object_path" "$interface_name" \
    SetOutputDevice s 'bluez_output.00_11_22_33_44_55.1' >/dev/null
active=$(busctl --user call "$service_name" "$object_path" "$interface_name" GetActiveDevice)
[[ $active == *'bluez_output.00_11_22_33_44_55.1'* ]]
[[ $(<"$default_sink") == 'bluez_output.00_11_22_33_44_55.1' ]]

busctl --user call "$service_name" "$object_path" "$interface_name" \
    SetDevicePreference sb 'bluez_output.00_11_22_33_44_55.1' false >/dev/null
busctl --user call "$service_name" "$object_path" "$interface_name" \
    SetDeviceTrusted sb 'bluez_output.00_11_22_33_44_55.1' true >/dev/null

printf '%s\n' 'sol-audiod D-Bus validation passed.'
