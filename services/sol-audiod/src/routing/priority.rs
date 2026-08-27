use super::device_type::AudioDeviceType;
use std::time::{Duration, Instant};

/// Dynamic priority modifiers based on context
#[derive(Debug, Clone, Copy)]
pub enum PriorityModifier {
    /// User explicitly selected this device recently
    UserExplicitChoice,
    /// Device was used in the last 30 minutes
    RecentlyUsed,
    /// Device is marked as trusted/favorite
    TrustedDevice,
    /// Device battery below 15%
    LowBattery,
    /// Active phone/video call
    InCall,
    /// Screen mirroring/casting active
    ScreenMirroring,
    /// Multiple users detected (meeting/party)
    MultipleUsers,
}

impl PriorityModifier {
    pub fn value(&self) -> i16 {
        match self {
            Self::UserExplicitChoice => 50,
            Self::RecentlyUsed => 10,
            Self::TrustedDevice => 5,
            Self::LowBattery => -20,
            Self::InCall => 30,
            Self::ScreenMirroring => -15,
            Self::MultipleUsers => -25,
        }
    }
}

/// Device priority with base + modifiers
#[derive(Debug, Clone)]
pub struct DevicePriority {
    pub base: u8,
    pub modifiers: Vec<PriorityModifier>,
}

impl DevicePriority {
    pub fn new(device_type: AudioDeviceType) -> Self {
        Self {
            base: device_type.base_priority(),
            modifiers: Vec::new(),
        }
    }

    pub fn add_modifier(&mut self, modifier: PriorityModifier) {
        self.modifiers.push(modifier);
    }

    pub fn effective_priority(&self) -> u8 {
        let total: i16 = self.base as i16 + self.modifiers.iter().map(|m| m.value()).sum::<i16>();
        total.clamp(0, 255) as u8
    }
}

/// Context for routing decisions
#[derive(Debug, Default)]
pub struct RoutingContext {
    pub is_in_call: bool,
    pub is_screen_mirroring: bool,
    pub multiple_users_detected: bool,
    pub recent_manual_selection: Option<(String, Instant)>, // (device_id, when)
}

impl RoutingContext {
    pub fn was_manually_selected_within(&self, device_id: &str, duration: Duration) -> bool {
        if let Some((id, instant)) = &self.recent_manual_selection {
            id == device_id && instant.elapsed() < duration
        } else {
            false
        }
    }

    pub fn mark_manual_selection(&mut self, device_id: String) {
        self.recent_manual_selection = Some((device_id, Instant::now()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_priorities() {
        assert!(
            AudioDeviceType::WiredHeadphones.base_priority()
                > AudioDeviceType::Headphones.base_priority()
        );
        assert!(
            AudioDeviceType::Headphones.base_priority() > AudioDeviceType::Speaker.base_priority()
        );
        assert!(
            AudioDeviceType::Speaker.base_priority()
                > AudioDeviceType::BuiltinSpeaker.base_priority()
        );
    }

    #[test]
    fn test_priority_modifiers() {
        let mut priority = DevicePriority::new(AudioDeviceType::Headphones);
        assert_eq!(priority.effective_priority(), 75);

        priority.add_modifier(PriorityModifier::UserExplicitChoice);
        assert_eq!(priority.effective_priority(), 125);

        priority.add_modifier(PriorityModifier::LowBattery);
        assert_eq!(priority.effective_priority(), 105);
    }

    #[test]
    fn test_priority_clamping() {
        let mut priority = DevicePriority::new(AudioDeviceType::BuiltinSpeaker);
        priority.add_modifier(PriorityModifier::MultipleUsers);
        priority.add_modifier(PriorityModifier::LowBattery);

        // Should clamp to 0, not go negative
        assert_eq!(priority.effective_priority(), 0);
    }
}
