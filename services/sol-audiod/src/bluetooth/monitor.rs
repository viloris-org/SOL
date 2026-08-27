use super::classifier::{classify_from_cod, classify_from_name};
use crate::routing::device_type::AudioDeviceType;
use anyhow::Result;
use bluer::{Device as BluerDevice, Session, Uuid};
use std::collections::HashMap;
use std::time::SystemTime;
use tracing::{debug, info, warn};

/// Bluetooth device metadata and classification
#[derive(Debug, Clone)]
pub struct BluetoothDevice {
    pub address: String,
    pub name: Option<String>,
    pub device_type: AudioDeviceType,
    pub classification_source: ClassificationSource,
    pub is_connected: bool,
    pub battery_level: Option<u8>,
    pub last_connected: Option<SystemTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassificationSource {
    VendorDatabase,
    DeviceClass,
    NamePattern,
    AudioProfiles,
    UserManual,
    Unknown,
}

impl std::fmt::Display for ClassificationSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VendorDatabase => write!(f, "vendor_db"),
            Self::DeviceClass => write!(f, "device_class"),
            Self::NamePattern => write!(f, "name_pattern"),
            Self::AudioProfiles => write!(f, "audio_profiles"),
            Self::UserManual => write!(f, "user_manual"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Device classifier with multiple signal sources
#[derive(Default)]
pub struct DeviceClassifier {
    manual_overrides: HashMap<String, AudioDeviceType>,
}

impl DeviceClassifier {
    pub fn new() -> Self {
        Self {
            manual_overrides: HashMap::new(),
        }
    }

    pub fn load_overrides(&mut self, overrides: HashMap<String, AudioDeviceType>) {
        self.manual_overrides = overrides;
    }

    pub fn set_manual_classification(&mut self, address: String, device_type: AudioDeviceType) {
        info!("Manual classification: {} -> {}", address, device_type);
        self.manual_overrides.insert(address, device_type);
    }

    /// Classify a Bluetooth device using multiple signal sources
    pub async fn classify(&self, device: &BluerDevice) -> Result<BluetoothDevice> {
        let address = device.address().to_string();
        let name = device.name().await.ok().flatten();

        // Priority 1: User manual override
        if let Some(&device_type) = self.manual_overrides.get(&address) {
            debug!(
                "Device {} classified via manual override: {}",
                address, device_type
            );
            return Ok(BluetoothDevice {
                address,
                name: name.clone(),
                device_type,
                classification_source: ClassificationSource::UserManual,
                is_connected: device.is_connected().await?,
                battery_level: device.battery_percentage().await.ok().flatten(),
                last_connected: None,
            });
        }

        // Priority 2: Vendor database (most accurate)
        // Note: BlueZ Device doesn't expose vendor_id/product_id directly
        // This would require parsing modalias or manufacturer data
        // TODO: Implement vendor ID extraction from device properties

        // Priority 3: Bluetooth Class of Device
        if let Ok(Some(class)) = device.class().await
            && let Some(device_type) = classify_from_cod(class)
        {
            debug!("Device {} classified via CoD: {}", address, device_type);
            return Ok(BluetoothDevice {
                address,
                name: name.clone(),
                device_type,
                classification_source: ClassificationSource::DeviceClass,
                is_connected: device.is_connected().await?,
                battery_level: device.battery_percentage().await.ok().flatten(),
                last_connected: None,
            });
        }

        // Priority 4: Device name pattern matching
        if let Some(ref device_name) = name
            && let Some(device_type) = classify_from_name(device_name)
        {
            debug!(
                "Device {} classified via name pattern: {}",
                address, device_type
            );
            return Ok(BluetoothDevice {
                address,
                name: name.clone(),
                device_type,
                classification_source: ClassificationSource::NamePattern,
                is_connected: device.is_connected().await?,
                battery_level: device.battery_percentage().await.ok().flatten(),
                last_connected: None,
            });
        }

        // Priority 5: Audio profiles heuristic
        if let Ok(Some(uuids)) = device.uuids().await
            && let Some(device_type) =
                classify_from_profiles(&uuids.iter().copied().collect::<Vec<_>>())
        {
            debug!(
                "Device {} classified via audio profiles: {}",
                address, device_type
            );
            return Ok(BluetoothDevice {
                address,
                name: name.clone(),
                device_type,
                classification_source: ClassificationSource::AudioProfiles,
                is_connected: device.is_connected().await?,
                battery_level: device.battery_percentage().await.ok().flatten(),
                last_connected: None,
            });
        }

        // Fallback: Unknown device type
        warn!(
            "Device {} could not be classified, marking as Unknown",
            address
        );
        Ok(BluetoothDevice {
            address,
            name,
            device_type: AudioDeviceType::Unknown,
            classification_source: ClassificationSource::Unknown,
            is_connected: device.is_connected().await?,
            battery_level: device.battery_percentage().await.ok().flatten(),
            last_connected: None,
        })
    }
}

/// Classify device from Bluetooth audio profile UUIDs
fn classify_from_profiles(uuids: &[Uuid]) -> Option<AudioDeviceType> {
    // Standard Bluetooth UUIDs
    const HFP_UUID: Uuid = Uuid::from_u128(0x0000111e_0000_1000_8000_00805f9b34fb); // Hands-Free Profile
    const HSP_UUID: Uuid = Uuid::from_u128(0x00001108_0000_1000_8000_00805f9b34fb); // Headset Profile
    const A2DP_SINK_UUID: Uuid = Uuid::from_u128(0x0000110b_0000_1000_8000_00805f9b34fb); // A2DP Sink
    let has_hfp = uuids.contains(&HFP_UUID);
    let has_hsp = uuids.contains(&HSP_UUID);
    let has_a2dp_sink = uuids.contains(&A2DP_SINK_UUID);

    match (has_hfp, has_hsp, has_a2dp_sink) {
        // HFP/HSP + A2DP = headphones (call support indicates personal device)
        (true, _, true) | (_, true, true) => Some(AudioDeviceType::Headphones),

        // A2DP only (no call support) = likely speaker
        (false, false, true) => Some(AudioDeviceType::Speaker),

        _ => None,
    }
}

/// Monitor for Bluetooth device connections/disconnections
pub struct BluetoothMonitor {
    session: Session,
    classifier: DeviceClassifier,
}

impl BluetoothMonitor {
    pub async fn new() -> Result<Self> {
        let session = Session::new().await?;
        Ok(Self {
            session,
            classifier: DeviceClassifier::new(),
        })
    }

    pub fn set_classifier(&mut self, classifier: DeviceClassifier) {
        self.classifier = classifier;
    }

    /// List all paired audio devices
    pub async fn list_audio_devices(&self) -> Result<Vec<BluetoothDevice>> {
        let adapter = self.session.default_adapter().await?;
        let device_addresses = adapter.device_addresses().await?;

        let mut devices = Vec::new();
        for addr in device_addresses {
            let device = adapter.device(addr)?;

            // Only include devices with audio UUIDs
            if let Ok(Some(uuids)) = device.uuids().await
                && has_audio_profile(&uuids.iter().copied().collect::<Vec<_>>())
            {
                match self.classifier.classify(&device).await {
                    Ok(bt_device) => devices.push(bt_device),
                    Err(e) => warn!("Failed to classify device {}: {}", addr, e),
                }
            }
        }

        Ok(devices)
    }

    /// Check if a device connection is likely for charging (auto-connect on case open)
    pub async fn is_charging_connection(&self, address: &str) -> bool {
        let adapter = match self.session.default_adapter().await {
            Ok(a) => a,
            Err(_) => return false,
        };

        let addr = match address.parse() {
            Ok(a) => a,
            Err(_) => return false,
        };

        let device = match adapter.device(addr) {
            Ok(d) => d,
            Err(_) => return false,
        };

        // Check if device is charging (if API supports it)
        // Note: Not all Bluetooth devices expose charging state
        // Heuristic: Recently connected (<2s) and no active audio streams
        if let Ok(true) = device.is_connected().await {
            // TODO: Check for active audio streams via PipeWire
            // For now, return false (assume user intent to use)
            return false;
        }

        false
    }
}

fn has_audio_profile(uuids: &[Uuid]) -> bool {
    const AUDIO_UUIDS: &[Uuid] = &[
        Uuid::from_u128(0x0000110b_0000_1000_8000_00805f9b34fb), // A2DP Sink
        Uuid::from_u128(0x0000110a_0000_1000_8000_00805f9b34fb), // A2DP Source
        Uuid::from_u128(0x0000111e_0000_1000_8000_00805f9b34fb), // HFP
        Uuid::from_u128(0x00001108_0000_1000_8000_00805f9b34fb), // HSP
    ];

    uuids.iter().any(|uuid| AUDIO_UUIDS.contains(uuid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_from_profiles() {
        // Headphones with HFP + A2DP
        let headphone_uuids = vec![
            Uuid::from_u128(0x0000111e_0000_1000_8000_00805f9b34fb), // HFP
            Uuid::from_u128(0x0000110b_0000_1000_8000_00805f9b34fb), // A2DP
        ];
        assert_eq!(
            classify_from_profiles(&headphone_uuids),
            Some(AudioDeviceType::Headphones)
        );

        // Speaker with only A2DP
        let speaker_uuids = vec![
            Uuid::from_u128(0x0000110b_0000_1000_8000_00805f9b34fb), // A2DP only
        ];
        assert_eq!(
            classify_from_profiles(&speaker_uuids),
            Some(AudioDeviceType::Speaker)
        );
    }
}
