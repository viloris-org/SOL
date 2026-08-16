use sol_shell::pipewire_audio::PipewireAudioProvider;
use sol_shell::topbar::{AudioProvider, ProviderState};

#[test]
#[ignore = "requires a live PipeWire Pulse compatibility service and pactl"]
fn top_bar_audio_provider_reads_the_live_default_pipewire_sink() {
    match PipewireAudioProvider::default().audio() {
        ProviderState::Available { value, stale } => {
            assert!(value.volume_percent <= 100);
            assert!(!stale);
        }
        ProviderState::Unavailable => {
            // A running PipeWire service may legitimately have no output sink.
        }
        ProviderState::Error(error) => panic!("live PipeWire query failed: {error}"),
    }
}
