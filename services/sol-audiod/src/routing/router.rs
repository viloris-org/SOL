use super::device_type::AudioDeviceType;
use super::priority::{DevicePriority, PriorityModifier, RoutingContext};
use crate::bluetooth::BluetoothDevice;
use anyhow::Result;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use tracing::{debug, info, warn};

/// Audio device in the routing system
#[derive(Debug, Clone)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub device_type: AudioDeviceType,
    pub is_connected: bool,
    pub battery_level: Option<u8>,
    pub is_charging: bool,
    pub last_used: Option<SystemTime>,
    pub trusted: bool,
}

impl From<BluetoothDevice> for AudioDevice {
    fn from(bt: BluetoothDevice) -> Self {
        Self {
            id: bt.address.clone(),
            name: bt.name.unwrap_or_else(|| bt.address.clone()),
            device_type: bt.device_type,
            is_connected: bt.is_connected,
            battery_level: bt.battery_level,
            is_charging: false,
            last_used: bt.last_connected,
            trusted: false,
        }
    }
}

/// Transition mode for switching audio devices
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionMode {
    /// Immediate switch (device disappeared/failed)
    Immediate,
    /// Smooth crossfade (user-initiated or auto-switch)
    Crossfade(Duration),
    /// Pause playback during switch
    Pause,
}

/// Audio router - manages device switching and prioritization
pub struct AudioRouter {
    /// All known devices
    devices: HashMap<String, AudioDevice>,
    /// Currently active output device
    active_device: Option<String>,
    /// Routing context (call state, screen mirroring, etc.)
    context: RoutingContext,
    /// Configuration
    config: RouterConfig,
}

#[derive(Debug, Clone)]
pub struct RouterConfig {
    /// Auto-switch for headphones
    pub auto_switch_headphones: bool,
    /// Auto-switch for speakers
    pub auto_switch_speakers: bool,
    /// Auto-switch for wired devices
    pub auto_switch_wired: bool,
    /// Crossfade duration in milliseconds
    pub crossfade_duration_ms: u64,
    /// Detect shared usage scenarios
    pub detect_shared_usage: bool,
    /// Battery-aware routing
    pub battery_aware: bool,
    /// Priority boosts per device
    pub priority_boosts: HashMap<String, i16>,
    /// Runtime/per-device auto-switch choices.
    pub auto_switch_overrides: HashMap<String, bool>,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            auto_switch_headphones: true,
            auto_switch_speakers: false,
            auto_switch_wired: true,
            crossfade_duration_ms: 300,
            detect_shared_usage: true,
            battery_aware: true,
            priority_boosts: HashMap::new(),
            auto_switch_overrides: HashMap::new(),
        }
    }
}

impl AudioRouter {
    pub fn new(config: RouterConfig) -> Self {
        Self {
            devices: HashMap::new(),
            active_device: None,
            context: RoutingContext::default(),
            config,
        }
    }

    /// Register a device with the router
    pub fn register_device(&mut self, device: AudioDevice) {
        info!(
            "Registering device: {} ({})",
            device.name, device.device_type
        );
        self.devices.insert(device.id.clone(), device);
    }

    /// Update backend metadata without discarding user-owned device state.
    pub fn upsert_device(&mut self, mut device: AudioDevice) {
        if let Some(existing) = self.devices.get(&device.id) {
            device.trusted = existing.trusted;
            device.last_used = existing.last_used;
        }
        self.register_device(device);
    }

    /// Unregister a device
    pub fn unregister_device(&mut self, device_id: &str) {
        if let Some(device) = self.devices.remove(device_id) {
            info!("Unregistered device: {}", device.name);
        }
        if self.active_device.as_deref() == Some(device_id) {
            self.active_device = None;
        }
    }

    /// Get currently active device
    pub fn active_device(&self) -> Option<&AudioDevice> {
        self.active_device
            .as_ref()
            .and_then(|id| self.devices.get(id))
    }

    pub fn device(&self, device_id: &str) -> Option<&AudioDevice> {
        self.devices.get(device_id)
    }

    pub fn devices(&self) -> impl Iterator<Item = &AudioDevice> {
        self.devices.values()
    }

    /// Calculate effective priority for a device
    pub fn calculate_priority(&self, device: &AudioDevice) -> u8 {
        let mut priority = DevicePriority::new(device.device_type);

        // Apply user manual selection boost
        if self
            .context
            .was_manually_selected_within(&device.id, Duration::from_secs(300))
        {
            priority.add_modifier(PriorityModifier::UserExplicitChoice);
        }

        // Apply recently used boost
        if let Some(last_used) = device.last_used
            && let Ok(elapsed) = SystemTime::now().duration_since(last_used)
            && elapsed < Duration::from_secs(1800)
        {
            priority.add_modifier(PriorityModifier::RecentlyUsed);
        }

        // Apply trusted device boost
        if device.trusted {
            priority.add_modifier(PriorityModifier::TrustedDevice);
        }

        // Apply battery penalty
        if self.config.battery_aware
            && let Some(battery) = device.battery_level
            && battery <= 15
        {
            priority.add_modifier(PriorityModifier::LowBattery);
        }

        // Apply call context
        if self.context.is_in_call && device.device_type.supports_calls() {
            priority.add_modifier(PriorityModifier::InCall);
        }

        // Apply screen mirroring context
        if self.context.is_screen_mirroring && device.device_type == AudioDeviceType::HDMI {
            priority.add_modifier(PriorityModifier::ScreenMirroring);
        }

        // Apply multiple users context
        if self.config.detect_shared_usage && self.context.multiple_users_detected {
            priority.add_modifier(PriorityModifier::MultipleUsers);
        }

        // Apply custom priority boost
        if let Some(&boost) = self.config.priority_boosts.get(&device.id) {
            let total = priority.effective_priority() as i16 + boost;
            return total.clamp(0, 255) as u8;
        }

        priority.effective_priority()
    }

    /// Check if auto-switch should happen for this device
    pub fn should_auto_switch(&self, new_device: &AudioDevice) -> bool {
        if let Some(enabled) = self.config.auto_switch_overrides.get(&new_device.id)
            && !enabled
        {
            debug!("Auto-switch disabled for device: {}", new_device.id);
            return false;
        }
        // Check if auto-switch is enabled for this device type
        let auto_switch_enabled = match new_device.device_type {
            AudioDeviceType::WiredHeadphones | AudioDeviceType::WiredSpeaker => {
                self.config.auto_switch_wired
            }
            AudioDeviceType::Earbuds | AudioDeviceType::Headphones => {
                self.config.auto_switch_headphones
            }
            AudioDeviceType::Speaker | AudioDeviceType::Soundbar => {
                self.config.auto_switch_speakers
            }
            _ => false,
        };

        if !auto_switch_enabled {
            debug!(
                "Auto-switch disabled for device type: {}",
                new_device.device_type
            );
            return false;
        }

        // Get current device
        let current = match self.active_device() {
            Some(d) => d,
            None => {
                debug!("No active device, allowing switch");
                return true;
            }
        };

        // Calculate priorities
        let new_priority = self.calculate_priority(new_device);
        let current_priority = self.calculate_priority(current);

        if new_priority <= current_priority {
            debug!(
                "New device priority ({}) <= current priority ({}), not switching",
                new_priority, current_priority
            );
            return false;
        }

        // Exception: Don't switch during active call to non-call device
        if self.context.is_in_call && !new_device.device_type.supports_calls() {
            debug!("In call, not switching to device without call support");
            return false;
        }

        // Exception: Don't switch away from HDMI during screen mirroring
        if self.context.is_screen_mirroring && current.device_type == AudioDeviceType::HDMI {
            debug!("Screen mirroring active, keeping HDMI device");
            return false;
        }

        // Exception: Don't switch to headphones during multi-user scenario
        if self.config.detect_shared_usage
            && self.context.multiple_users_detected
            && new_device.device_type.is_personal()
        {
            debug!("Multiple users detected, not switching to personal device");
            return false;
        }

        // Exception: Don't switch if user manually selected current device recently
        if self
            .context
            .was_manually_selected_within(&current.id, Duration::from_secs(300))
        {
            debug!("User manually selected current device recently, not auto-switching");
            return false;
        }

        // Exception: Charging connection detection (heuristic)
        if new_device.is_charging {
            debug!("Device appears to be charging, not auto-switching");
            return false;
        }

        true
    }

    /// Handle device connection event
    pub async fn handle_device_connected(
        &mut self,
        device: AudioDevice,
    ) -> Result<Option<TransitionMode>> {
        info!("Device connected: {} ({})", device.name, device.device_type);

        let device_id = device.id.clone();
        self.register_device(device);

        // Check if we should auto-switch
        let device = self.devices.get(&device_id).unwrap();
        if self.should_auto_switch(device) {
            info!("Auto-switching to device: {}", device.name);
            self.activate_device(&device_id, false)?;
            let duration = Duration::from_millis(self.config.crossfade_duration_ms);
            Ok(Some(TransitionMode::Crossfade(duration)))
        } else {
            info!("Device connected but not auto-switching: {}", device.name);
            // TODO: Send notification that device is available
            Ok(None)
        }
    }

    /// Handle device disconnection event
    pub async fn handle_device_disconnected(
        &mut self,
        device_id: &str,
    ) -> Result<Option<(String, TransitionMode)>> {
        info!("Device disconnected: {}", device_id);

        // If this was the active device, switch to next best device
        if self.active_device.as_deref() == Some(device_id) {
            self.unregister_device(device_id);

            // Find fallback device
            if let Some(fallback_id) = self.find_best_device() {
                info!("Switching to fallback device: {}", fallback_id);
                self.activate_device(&fallback_id, false)?;
                Ok(Some((fallback_id, TransitionMode::Immediate)))
            } else {
                warn!("No fallback device available");
                Ok(None)
            }
        } else {
            self.unregister_device(device_id);
            Ok(None)
        }
    }

    /// Manually switch to a specific device
    pub fn switch_to_device(&mut self, device_id: &str) -> Result<()> {
        self.activate_device(device_id, true)
    }

    /// Adopt a backend/automatic route without creating a manual-selection lock.
    pub fn activate_device(&mut self, device_id: &str, manual: bool) -> Result<()> {
        let Some(device) = self.devices.get(device_id) else {
            anyhow::bail!("Device not found: {}", device_id);
        };
        if !device.is_connected {
            anyhow::bail!("Device is disconnected: {}", device_id);
        }

        info!("Switching to device: {}", device_id);
        self.active_device = Some(device_id.to_string());
        if manual {
            self.context.mark_manual_selection(device_id.to_string());
        }

        // AudioControl changes the backend first, then commits this policy
        // state. Keeping the router backend-free makes failures testable.
        Ok(())
    }

    /// Find the best available device based on priority
    pub fn find_best_device(&self) -> Option<String> {
        self.devices
            .values()
            .filter(|d| d.is_connected)
            .max_by_key(|d| self.calculate_priority(d))
            .map(|d| d.id.clone())
    }

    /// List all connected devices sorted by priority
    pub fn list_devices_by_priority(&self) -> Vec<(String, u8)> {
        let mut devices: Vec<_> = self
            .devices
            .values()
            .filter(|d| d.is_connected)
            .map(|d| (d.id.clone(), self.calculate_priority(d)))
            .collect();

        devices.sort_by_key(|(_, priority)| std::cmp::Reverse(*priority));
        devices
    }

    /// Update routing context
    pub fn update_context(&mut self, context: RoutingContext) {
        self.context = context;
    }

    /// Set device as trusted
    pub fn set_device_trusted(&mut self, device_id: &str, trusted: bool) {
        if let Some(device) = self.devices.get_mut(device_id) {
            device.trusted = trusted;
            info!("Device {} trusted status set to {}", device_id, trusted);
        }
    }

    pub fn set_device_auto_switch(&mut self, device_id: &str, enabled: bool) -> Result<()> {
        if !self.devices.contains_key(device_id) {
            anyhow::bail!("Device not found: {}", device_id);
        }
        self.config
            .auto_switch_overrides
            .insert(device_id.to_owned(), enabled);
        Ok(())
    }

    pub fn set_device_priority_boost(&mut self, device_id: &str, boost: i16) -> Result<()> {
        if !self.devices.contains_key(device_id) {
            anyhow::bail!("Device not found: {}", device_id);
        }
        self.config
            .priority_boosts
            .insert(device_id.to_owned(), boost);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_device(id: &str, device_type: AudioDeviceType) -> AudioDevice {
        AudioDevice {
            id: id.to_string(),
            name: format!("Test {}", id),
            device_type,
            is_connected: true,
            battery_level: None,
            is_charging: false,
            last_used: None,
            trusted: false,
        }
    }

    #[test]
    fn test_headphone_priority_over_speaker() {
        let config = RouterConfig::default();
        let mut router = AudioRouter::new(config);

        let speaker = create_test_device("speaker", AudioDeviceType::Speaker);
        let headphones = create_test_device("headphones", AudioDeviceType::Headphones);

        router.register_device(speaker.clone());
        router.register_device(headphones.clone());

        let speaker_priority = router.calculate_priority(&speaker);
        let headphones_priority = router.calculate_priority(&headphones);

        assert!(headphones_priority > speaker_priority);
    }

    #[test]
    fn test_battery_penalty() {
        let config = RouterConfig {
            battery_aware: true,
            ..Default::default()
        };
        let router = AudioRouter::new(config);

        let mut low_battery = create_test_device("low", AudioDeviceType::Headphones);
        low_battery.battery_level = Some(10);

        let mut good_battery = create_test_device("good", AudioDeviceType::Headphones);
        good_battery.battery_level = Some(80);

        let low_priority = router.calculate_priority(&low_battery);
        let good_priority = router.calculate_priority(&good_battery);

        assert!(good_priority > low_priority);
    }

    #[test]
    fn test_manual_selection_boost() {
        let config = RouterConfig::default();
        let mut router = AudioRouter::new(config);

        let device = create_test_device("dev", AudioDeviceType::Speaker);
        router.register_device(device.clone());

        let base_priority = router.calculate_priority(&device);

        router.context.mark_manual_selection("dev".to_string());
        let boosted_priority = router.calculate_priority(&device);

        assert!(boosted_priority > base_priority);
    }
}
