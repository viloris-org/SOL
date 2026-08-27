use crate::routing::device_type::AudioDeviceType;
use lazy_static::lazy_static;
use std::collections::HashMap;

/// Classify device from Bluetooth Class of Device (CoD)
/// The class is a u32 value from BlueZ Device.Class property
pub fn classify_from_cod(class: u32) -> Option<AudioDeviceType> {
    // Bluetooth CoD spec: https://www.bluetooth.com/specifications/assigned-numbers/
    let major = ((class >> 8) & 0x1F) as u8;
    let minor = ((class >> 2) & 0x3F) as u8;

    // Major class 0x04 = Audio/Video
    if major != 0x04 {
        return None;
    }

    match minor {
        0x01 => Some(AudioDeviceType::Headphones), // Wearable Headset
        0x02 => Some(AudioDeviceType::Earbuds),    // Hands-free
        0x05 => Some(AudioDeviceType::Speaker),    // Loudspeaker
        0x06 => Some(AudioDeviceType::Headphones), // Headphones
        0x0B => Some(AudioDeviceType::Speaker),    // VCR (often speakers)
        0x14 => Some(AudioDeviceType::Speaker),    // HiFi Audio Device
        _ => None,
    }
}

/// Classify device from name patterns
pub fn classify_from_name(name: &str) -> Option<AudioDeviceType> {
    let lower = name.to_lowercase();

    // Headphone patterns
    const HEADPHONE_PATTERNS: &[&str] = &[
        "headphone",
        "headset",
        "earphone",
        "earbud",
        "airpod",
        "buds",
        "galaxy buds",
        "wh-",
        "wf-", // Sony naming (WH-1000XM5, WF-1000XM4)
        "quietcomfort",
        "soundlink", // Bose
        "momentum",  // Sennheiser
        "solo",
        "studio", // Beats
        "mdr-",   // Sony MDR series
    ];

    // Speaker patterns
    const SPEAKER_PATTERNS: &[&str] = &[
        "speaker",
        "soundbar",
        "sound bar",
        "homepod",
        "echo",
        "nest audio",
        "nest mini",
        "boom",
        "megaboom",
        "wonderboom", // UE
        "charge",
        "flip",
        "pulse",
        "xtreme",    // JBL
        "srs-",      // Sony speaker series
        "soundcore", // Anker
    ];

    for pattern in HEADPHONE_PATTERNS {
        if lower.contains(pattern) {
            return Some(AudioDeviceType::Headphones);
        }
    }

    for pattern in SPEAKER_PATTERNS {
        if lower.contains(pattern) {
            return Some(AudioDeviceType::Speaker);
        }
    }

    None
}

// Classify devices from vendor and product IDs.
lazy_static! {
    static ref VENDOR_DATABASE: HashMap<(u16, u16), AudioDeviceType> = {
        let mut m = HashMap::new();

        // Apple (Vendor ID: 0x004C)
        m.insert((0x004C, 0x2002), AudioDeviceType::Earbuds);  // AirPods Pro
        m.insert((0x004C, 0x200A), AudioDeviceType::Headphones); // AirPods Max
        m.insert((0x004C, 0x200F), AudioDeviceType::Earbuds);  // AirPods Pro 2
        m.insert((0x004C, 0x2012), AudioDeviceType::Earbuds);  // AirPods 3

        // Sony (Vendor ID: 0x054C)
        m.insert((0x054C, 0x0CE0), AudioDeviceType::Headphones); // WH-1000XM4
        m.insert((0x054C, 0x0CE1), AudioDeviceType::Headphones); // WH-1000XM5
        m.insert((0x054C, 0x0D08), AudioDeviceType::Earbuds);    // WF-1000XM4
        m.insert((0x054C, 0x0D09), AudioDeviceType::Earbuds);    // WF-1000XM5
        m.insert((0x054C, 0x0D0A), AudioDeviceType::Speaker);    // SRS-XB43
        m.insert((0x054C, 0x0D0B), AudioDeviceType::Speaker);    // SRS-XB33

        // Bose (Vendor ID: 0x0A12)
        m.insert((0x0A12, 0x4021), AudioDeviceType::Headphones); // QuietComfort 35 II
        m.insert((0x0A12, 0x4025), AudioDeviceType::Headphones); // QuietComfort 45
        m.insert((0x0A12, 0x4031), AudioDeviceType::Earbuds);    // QuietComfort Earbuds

        // Samsung (Vendor ID: 0x0075)
        m.insert((0x0075, 0x0001), AudioDeviceType::Earbuds);    // Galaxy Buds
        m.insert((0x0075, 0x0002), AudioDeviceType::Earbuds);    // Galaxy Buds+
        m.insert((0x0075, 0x0003), AudioDeviceType::Earbuds);    // Galaxy Buds Live
        m.insert((0x0075, 0x0004), AudioDeviceType::Earbuds);    // Galaxy Buds Pro

        // JBL (Vendor ID: 0x0A12, shared with CSR chipsets)
        m.insert((0x0A12, 0x0001), AudioDeviceType::Speaker);    // Flip/Charge series

        m
    };
}

pub fn classify_from_vendor(vendor_id: u16, product_id: u16) -> Option<AudioDeviceType> {
    VENDOR_DATABASE.get(&(vendor_id, product_id)).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_classification_headphones() {
        assert_eq!(
            classify_from_name("Sony WH-1000XM5"),
            Some(AudioDeviceType::Headphones)
        );
        assert_eq!(
            classify_from_name("AirPods Pro"),
            Some(AudioDeviceType::Headphones)
        );
        assert_eq!(
            classify_from_name("Bose QuietComfort 45"),
            Some(AudioDeviceType::Headphones)
        );
    }

    #[test]
    fn test_name_classification_speakers() {
        assert_eq!(
            classify_from_name("JBL Charge 5"),
            Some(AudioDeviceType::Speaker)
        );
        assert_eq!(
            classify_from_name("HomePod mini"),
            Some(AudioDeviceType::Speaker)
        );
        assert_eq!(
            classify_from_name("Sony SRS-XB43"),
            Some(AudioDeviceType::Speaker)
        );
    }

    #[test]
    fn test_vendor_database() {
        // Sony WH-1000XM5
        assert_eq!(
            classify_from_vendor(0x054C, 0x0CE1),
            Some(AudioDeviceType::Headphones)
        );

        // AirPods Pro
        assert_eq!(
            classify_from_vendor(0x004C, 0x2002),
            Some(AudioDeviceType::Earbuds)
        );
    }
}
