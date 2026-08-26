/// Example: Connect to a WiFi network
///
/// This demonstrates the high-level API for WiFi connection.
/// Run with: cargo run --example wifi_connect

use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <ssid> [passphrase]", args[0]);
        eprintln!("Example: {} MyNetwork mypassword", args[0]);
        std::process::exit(1);
    }

    let ssid = args[1].clone();
    let passphrase = args.get(2).cloned();

    println!("Connecting to WiFi network: {}", ssid);
    if passphrase.is_some() {
        println!("Using WPA2 authentication");
    } else {
        println!("Using open network");
    }

    // In a real scenario, this would connect via D-Bus to the running daemon
    // For this example, we'll show the API usage pattern

    println!("\nD-Bus command to connect:");
    if let Some(pass) = passphrase {
        println!("  busctl call org.sol.Network1 /org/sol/Network1 \\");
        println!("    org.sol.Network1.Manager ConnectWifi ss \\");
        println!("    \"{}\" \"{}\"", ssid, pass);
    } else {
        println!("  busctl call org.sol.Network1 /org/sol/Network1 \\");
        println!("    org.sol.Network1.Manager ConnectWifi ss \\");
        println!("    \"{}\" \"\"", ssid);
    }

    println!("\nTo scan for networks:");
    println!("  busctl call org.sol.Network1 /org/sol/Network1 \\");
    println!("    org.sol.Network1.Manager ScanWifi");

    println!("\nTo check connection status:");
    println!("  busctl call org.sol.Network1 /org/sol/Network1 \\");
    println!("    org.sol.Network1.Manager ActiveConnection");

    Ok(())
}
