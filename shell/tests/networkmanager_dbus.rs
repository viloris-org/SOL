use sol_shell::networkmanager::NetworkManagerProvider;
use sol_shell::topbar::{NetworkProvider, ProviderState};

#[test]
#[ignore = "requires the host org.freedesktop.NetworkManager system-bus service"]
fn networkmanager_status_round_trip() {
    let provider = NetworkManagerProvider::connect_system()
        .expect("connect to live NetworkManager system service");
    match provider.network() {
        ProviderState::Available { .. } => {}
        ProviderState::Unavailable => panic!("NetworkManager unexpectedly unavailable"),
        ProviderState::Error(error) => panic!("live NetworkManager query failed: {error}"),
    }
}
