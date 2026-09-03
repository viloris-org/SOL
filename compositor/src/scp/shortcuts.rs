//! Global shortcut registration and conflict arbitration.

use crate::scp::protocol::{KeyBinding, SessionId, ShortcutId, ShortcutPriority};
use std::collections::HashMap;

/// A client cannot pin an unbounded number of global bindings.
pub const MAX_SHORTCUTS_PER_SESSION: usize = 64;
pub const MAX_SHORTCUT_JUSTIFICATION: usize = 512;

/// All modifier bits currently understood by the SCP keymap.
const KNOWN_MODIFIERS: u32 = 0xff;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShortcutRegistration {
    pub id: ShortcutId,
    pub owner: SessionId,
    pub binding: KeyBinding,
    pub priority: ShortcutPriority,
}

#[derive(Debug, Default)]
pub struct ShortcutManager {
    by_binding: HashMap<KeyBinding, ShortcutRegistration>,
    by_id: HashMap<ShortcutId, KeyBinding>,
    next_id: ShortcutId,
}

impl ShortcutManager {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            ..Self::default()
        }
    }

    pub fn register(
        &mut self,
        owner: SessionId,
        binding: KeyBinding,
        priority: ShortcutPriority,
        justification: &str,
    ) -> Result<(ShortcutRegistration, Option<ShortcutRegistration>), String> {
        Self::validate(binding, justification)?;
        if self.count_for(owner) >= MAX_SHORTCUTS_PER_SESSION {
            return Err(format!(
                "A session may register at most {MAX_SHORTCUTS_PER_SESSION} shortcuts"
            ));
        }

        if let Some(existing) = self.by_binding.get(&binding).copied() {
            if existing.owner == owner {
                return Ok((existing, None));
            }
            if existing.priority >= priority {
                return Err(format!(
                    "Shortcut is already owned at {:?} priority",
                    existing.priority
                ));
            }
        }

        let displaced = self.by_binding.remove(&binding);
        if let Some(displaced) = displaced {
            self.by_id.remove(&displaced.id);
        }

        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or("Shortcut ID space exhausted")?;
        let registration = ShortcutRegistration {
            id,
            owner,
            binding,
            priority,
        };
        self.by_binding.insert(binding, registration);
        self.by_id.insert(id, binding);
        Ok((registration, displaced))
    }

    pub fn unregister(&mut self, owner: SessionId, id: ShortcutId) -> Result<(), String> {
        let binding = *self.by_id.get(&id).ok_or("Shortcut not found")?;
        let registration = self
            .by_binding
            .get(&binding)
            .ok_or("Shortcut registry is inconsistent")?;
        if registration.owner != owner {
            return Err("Shortcut does not belong to this session".to_string());
        }
        self.by_id.remove(&id);
        self.by_binding.remove(&binding);
        Ok(())
    }

    pub fn matching(&self, binding: KeyBinding) -> Option<ShortcutRegistration> {
        self.by_binding.get(&binding).copied()
    }

    pub fn remove_session(&mut self, owner: SessionId) {
        let ids: Vec<_> = self
            .by_id
            .iter()
            .filter_map(|(&id, binding)| {
                self.by_binding
                    .get(binding)
                    .is_some_and(|registration| registration.owner == owner)
                    .then_some(id)
            })
            .collect();
        for id in ids {
            if let Some(binding) = self.by_id.remove(&id) {
                self.by_binding.remove(&binding);
            }
        }
    }

    pub fn count_for(&self, owner: SessionId) -> usize {
        self.by_binding
            .values()
            .filter(|registration| registration.owner == owner)
            .count()
    }

    fn validate(binding: KeyBinding, justification: &str) -> Result<(), String> {
        if binding.keycode < 8 || binding.keycode > 767 {
            return Err("Shortcut keycode is outside the XKB range".to_string());
        }
        if binding.modifiers == 0 || binding.modifiers & !KNOWN_MODIFIERS != 0 {
            return Err("Shortcut must use only known, non-empty modifiers".to_string());
        }
        let justification = justification.trim();
        if justification.is_empty() || justification.len() > MAX_SHORTCUT_JUSTIFICATION {
            return Err(format!(
                "Shortcut justification must contain 1..={MAX_SHORTCUT_JUSTIFICATION} bytes"
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scp::keymap::modifiers;

    fn binding() -> KeyBinding {
        KeyBinding {
            keycode: 24,
            modifiers: modifiers::SUPER,
        }
    }

    #[test]
    fn higher_priority_displaces_and_lower_priority_cannot_steal() {
        let mut manager = ShortcutManager::new();
        let (app, _) = manager
            .register(1, binding(), ShortcutPriority::App, "open app")
            .unwrap();
        let (shell, displaced) = manager
            .register(2, binding(), ShortcutPriority::Shell, "shell action")
            .unwrap();
        assert_eq!(displaced, Some(app));
        assert_eq!(manager.matching(binding()), Some(shell));
        assert!(
            manager
                .register(3, binding(), ShortcutPriority::App, "steal")
                .is_err()
        );
    }

    #[test]
    fn disconnect_releases_every_binding() {
        let mut manager = ShortcutManager::new();
        let (registration, _) = manager
            .register(7, binding(), ShortcutPriority::App, "test")
            .unwrap();
        manager.remove_session(7);
        assert_eq!(manager.matching(binding()), None);
        assert!(manager.unregister(7, registration.id).is_err());
    }
}
