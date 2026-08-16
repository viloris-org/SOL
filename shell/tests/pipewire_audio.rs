use sol_shell::pipewire_audio::{AudioDeviceInventoryProvider, PipewireAudioProvider};
use sol_shell::topbar::{AudioProvider, ProviderState};

#[test]
#[ignore = "requires a live PipeWire Pulse compatibility service and pactl"]
fn shell_audio_provider_reads_live_status_and_output_inventory() {
    let provider = PipewireAudioProvider::default();
    match provider.audio() {
        ProviderState::Available { value, stale } => {
            assert!(value.volume_percent <= 100);
            assert!(!stale);
        }
        ProviderState::Unavailable => {
            // A running PipeWire service may legitimately have no output sink.
        }
        ProviderState::Error(error) => panic!("live PipeWire query failed: {error}"),
    }
    match provider.output_devices() {
        ProviderState::Available { value, stale } => {
            assert!(!value.is_empty());
            assert!(!stale);
            assert!(value.iter().all(|device| !device.name.is_empty()));
            assert!(value.iter().all(|device| !device.description.is_empty()));
            assert!(value.iter().filter(|device| device.is_default).count() <= 1);
            assert!(
                value
                    .iter()
                    .flat_map(|device| &device.ports)
                    .all(|port| !port.name.is_empty() && !port.description.is_empty())
            );
        }
        ProviderState::Unavailable => {
            // A running PipeWire service may legitimately have no output devices.
        }
        ProviderState::Error(error) => panic!("live PipeWire inventory failed: {error}"),
    }
}
