use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WiFiProfile {
    pub ssid: String,
    pub bssid: Option<String>,  // MAC address of specific AP (optional)
    pub security: WiFiSecurity,
    pub passphrase_encrypted: Option<Vec<u8>>,  // Encrypted passphrase
    pub hidden: bool,
    pub priority: u32,  // Higher = preferred
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WiFiSecurity {
    Open,
    Wep,
    Wpa,
    Wpa2,
    Wpa3,
}

impl WiFiProfile {
    pub fn new(ssid: String, security: WiFiSecurity) -> Self {
        Self {
            ssid,
            bssid: None,
            security,
            passphrase_encrypted: None,
            hidden: false,
            priority: 0,
        }
    }
}
