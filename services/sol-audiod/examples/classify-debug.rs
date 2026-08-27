use sol_audiod::bluetooth::{classify_from_cod, classify_from_name, classify_from_vendor};
use sol_audiod::routing::AudioDeviceType;

fn main() {
    println!("=== SOL Audio Device Classifier Debug Tool ===\n");

    // Test Class of Device classification
    println!("--- Class of Device (CoD) Classification ---");
    test_cod(0x240404, "Expected: Headphones");
    test_cod(0x240408, "Expected: Hands-free (Earbuds)");
    test_cod(0x240414, "Expected: Loudspeaker");
    test_cod(0x240418, "Expected: Headphones");
    println!();

    // Test name pattern matching
    println!("--- Name Pattern Classification ---");
    test_name("Sony WH-1000XM5", "Expected: Headphones");
    test_name("AirPods Pro", "Expected: Headphones");
    test_name("Galaxy Buds Pro", "Expected: Headphones");
    test_name("JBL Charge 5", "Expected: Speaker");
    test_name("HomePod mini", "Expected: Speaker");
    test_name("Bose QuietComfort 45", "Expected: Headphones");
    test_name("Sony SRS-XB43", "Expected: Speaker");
    test_name("Unknown Device XYZ", "Expected: None");
    println!();

    // Test vendor database
    println!("--- Vendor Database Classification ---");
    test_vendor(0x004C, 0x2002, "AirPods Pro", "Expected: Earbuds");
    test_vendor(0x004C, 0x200A, "AirPods Max", "Expected: Headphones");
    test_vendor(0x054C, 0x0CE1, "Sony WH-1000XM5", "Expected: Headphones");
    test_vendor(0x054C, 0x0D08, "Sony WF-1000XM4", "Expected: Earbuds");
    test_vendor(0x054C, 0x0D0A, "Sony SRS-XB43", "Expected: Speaker");
    test_vendor(0x0A12, 0x4025, "Bose QC45", "Expected: Headphones");
    test_vendor(0x0075, 0x0004, "Galaxy Buds Pro", "Expected: Earbuds");
    println!();

    println!("=== Priority Rankings ===");
    for device_type in [
        AudioDeviceType::WiredHeadphones,
        AudioDeviceType::WiredSpeaker,
        AudioDeviceType::Earbuds,
        AudioDeviceType::Headphones,
        AudioDeviceType::CarAudio,
        AudioDeviceType::Speaker,
        AudioDeviceType::Soundbar,
        AudioDeviceType::HDMI,
        AudioDeviceType::BuiltinSpeaker,
    ] {
        println!(
            "{:20} priority: {}",
            format!("{}", device_type),
            device_type.base_priority()
        );
    }
}

fn test_cod(cod: u32, expected: &str) {
    let result = classify_from_cod(cod);
    println!("CoD 0x{:06X}: {:?} ({})", cod, result, expected);
}

fn test_name(name: &str, expected: &str) {
    let result = classify_from_name(name);
    println!("{:30} -> {:?} ({})", name, result, expected);
}

fn test_vendor(vid: u16, pid: u16, device: &str, expected: &str) {
    let result = classify_from_vendor(vid, pid);
    println!(
        "{:30} (VID={:#06x} PID={:#06x}) -> {:?} ({})",
        device, vid, pid, result, expected
    );
}
