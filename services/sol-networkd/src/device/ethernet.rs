use anyhow::Result;
use tracing::info;

use crate::device::Device;

/// Ethernet device implementation
pub struct EthernetDevice {
    device: Device,
}

impl EthernetDevice {
    pub fn new(device: Device) -> Self {
        Self { device }
    }

    pub async fn connect(&self) -> Result<()> {
        info!("Bringing up Ethernet interface: {}", self.device.interface);
        // TODO: Bring up interface, start DHCP
        Ok(())
    }

    pub async fn disconnect(&self) -> Result<()> {
        info!("Bringing down Ethernet interface: {}", self.device.interface);
        // TODO: Bring down interface
        Ok(())
    }

    pub fn is_carrier_detected(&self) -> bool {
        // TODO: Check if cable is plugged in (carrier detected)
        false
    }

    pub fn link_speed(&self) -> Option<u32> {
        // TODO: Get link speed in Mbps (e.g., 1000 for gigabit)
        None
    }
}
