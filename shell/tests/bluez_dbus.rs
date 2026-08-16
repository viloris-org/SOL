use sol_shell::bluez::BluezProvider;
use sol_shell::topbar::{BluetoothProvider, ProviderState};

#[test]
#[ignore = "requires the host org.bluez system-bus service"]
fn bluez_status_round_trip() {
    let provider = BluezProvider::connect_system().expect("connect to live BlueZ system service");
    match provider.bluetooth() {
        ProviderState::Available { value, stale } => {
            assert!(!value.adapters.is_empty());
            assert!(!stale);
            assert!(
                value
                    .devices
                    .iter()
                    .all(|device| device.battery_percent.is_none_or(|percent| percent <= 100))
            );
        }
        ProviderState::Unavailable => {
            // A running BlueZ service may legitimately have no local adapter.
        }
        ProviderState::Error(error) => panic!("live BlueZ query failed: {error}"),
    }
}
