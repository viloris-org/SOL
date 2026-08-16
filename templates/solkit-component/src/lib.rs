//! A reusable, renderer-independent SolUI component.

use sol_design::accessibility::TokenMode;
use sol_ui::{Button, SemanticControl, VisualTokenContract};

/// A reusable primary action built entirely from semantic SolUI contracts.
#[derive(Debug)]
pub struct ApplyAction {
    button: Button,
}

impl Default for ApplyAction {
    fn default() -> Self {
        Self::new()
    }
}

impl ApplyAction {
    /// Construct the component with its semantic intent only.
    #[must_use]
    pub fn new() -> Self {
        Self {
            button: Button::new().with_label("Apply"),
        }
    }

    /// Expose the component to SolUI focus and accessibility behavior.
    #[must_use]
    pub fn semantic_control(&self, id: impl Into<String>) -> SemanticControl {
        SemanticControl::button(id, &self.button)
    }

    /// Return the component's visual decisions as named design-token roles.
    #[must_use]
    pub fn visual_tokens(&self) -> VisualTokenContract {
        self.button.visual_tokens()
    }

    /// Resolve this component's semantic motion under active accessibility tokens.
    #[must_use]
    pub fn motion_duration_ms(&self, mode: TokenMode) -> u32 {
        mode.motion_spec(self.button.motion()).duration_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sol_ui::{InteractionTree, Key, KeyboardOutcome};

    #[test]
    fn component_uses_semantic_controls_and_token_only_visuals() {
        let component = ApplyAction::new();
        let mut interactions = InteractionTree::new("component", "Component fixture");
        interactions.push(component.semantic_control("apply"));

        assert!(matches!(
            interactions.handle_key(Key::Tab),
            KeyboardOutcome::FocusMoved(_)
        ));
        assert!(matches!(
            interactions.handle_key(Key::Enter),
            KeyboardOutcome::Activated(id) if id.as_str() == "apply"
        ));
        assert_eq!(
            component.visual_tokens().snapshot(),
            "background=Elevated;foreground=TextPrimary;padding=Md;radius=Sm;metric=Button;motion=Fast;typography=Label"
        );
    }

    #[test]
    fn component_respects_reduced_motion_through_token_mode() {
        assert_eq!(
            ApplyAction::new().motion_duration_ms(TokenMode::dark().reduced_motion()),
            0
        );
    }

    #[test]
    fn manifest_uses_only_public_component_dependencies() {
        let manifest = include_str!("../Cargo.toml");
        for forbidden in [
            "sol-app",
            "sol-system",
            "slint",
            "winit",
            "wayland",
            "smithay",
            "wgpu",
            "vulkan",
        ] {
            assert!(!manifest.contains(forbidden), "manifest leaked {forbidden}");
        }
    }
}
