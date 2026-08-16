use sol_shell::topbar::{PowerProvider, ProviderState};
use sol_shell::upower::UpowerPowerProvider;

#[test]
#[ignore = "requires the host org.freedesktop.UPower system-bus service"]
fn top_bar_power_provider_reads_the_live_upower_display_device() {
    let provider = UpowerPowerProvider::connect_system().expect("connect to live system UPower");
    match provider.power() {
        ProviderState::Available { value, stale } => {
            assert!(value.percent <= 100);
            assert!(!stale);
        }
        ProviderState::Unavailable => {
            // Desktops without a battery expose an absent aggregate display
            // device. The adapter must not fabricate a zero-percent battery.
        }
        ProviderState::Error(error) => panic!("live UPower query failed: {error}"),
    }
}
