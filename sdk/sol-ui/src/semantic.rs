//! Semantic interaction, focus, keyboard, and accessibility contracts.
//!
//! This module intentionally models behavior in SOL terms. Native renderers
//! consume the resulting semantic state but do not own focus traversal or
//! keyboard policy.

use sol_design::{
    color::Color, metrics::ControlMetric, motion::Motion, radius::Radius, spacing::Spacing,
    typography::FontStyle,
};

use crate::{Button, Tab, TextField};

/// Stable identifier for a semantic control.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SemanticId(String);

impl SemanticId {
    /// Create a semantic identifier from an application-owned stable name.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Return the stable name for diagnostics and accessibility bridges.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Semantic role exposed to keyboard and accessibility integrations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticRole {
    /// Non-interactive grouping node.
    Group,
    /// Activatable button.
    Button,
    /// Editable text input.
    TextField,
    /// Selectable tab.
    Tab,
}

/// State exposed for a semantic accessibility node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AccessibilityState {
    /// Whether keyboard focus is currently on this node.
    pub focused: bool,
    /// Whether the node refuses interaction.
    pub disabled: bool,
    /// Whether a selectable node is selected.
    pub selected: bool,
    /// Whether text content may be edited.
    pub editable: bool,
}

/// A renderer-independent accessibility node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessibilityNode {
    /// Stable semantic identifier.
    pub id: SemanticId,
    /// Exposed semantic role.
    pub role: SemanticRole,
    /// Human-readable accessible name.
    pub label: String,
    /// Current text value, when relevant.
    pub value: Option<String>,
    /// State consumed by a screen-reader bridge.
    pub state: AccessibilityState,
    /// Descendant semantic nodes.
    pub children: Vec<Self>,
}

/// The semantic token contract for a component's visual projection.
///
/// Its fields are roles rather than concrete dimensions, colors, or timings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualTokenContract {
    /// Semantic background color role.
    pub background: Color,
    /// Semantic foreground color role.
    pub foreground: Color,
    /// Named padding token.
    pub padding: Spacing,
    /// Named corner-radius token.
    pub radius: Radius,
    /// Named geometry token.
    pub metric: ControlMetric,
    /// Named motion token.
    pub motion: Motion,
    /// Named typography token.
    pub typography: FontStyle,
}

impl VisualTokenContract {
    /// Produce a stable, role-only snapshot suitable for a token-only test.
    pub fn snapshot(self) -> String {
        format!(
            "background={:?};foreground={:?};padding={:?};radius={:?};metric={:?};motion={:?};typography={:?}",
            self.background,
            self.foreground,
            self.padding,
            self.radius,
            self.metric,
            self.motion,
            self.typography,
        )
    }
}

/// Components that resolve appearance solely through design-token roles.
pub trait TokenizedComponent {
    /// Return every visual decision as a sol-design token role.
    fn visual_tokens(&self) -> VisualTokenContract;
}

impl TokenizedComponent for Button {
    fn visual_tokens(&self) -> VisualTokenContract {
        Self::visual_tokens(self)
    }
}

impl Button {
    /// Return this button's complete token-only visual contract.
    pub fn visual_tokens(&self) -> VisualTokenContract {
        VisualTokenContract {
            background: self.background(),
            foreground: self.foreground(),
            padding: self.padding_x(),
            radius: self.corner_radius(),
            metric: self.metric(),
            motion: self.motion(),
            typography: self.text_style(),
        }
    }
}

impl TokenizedComponent for TextField {
    fn visual_tokens(&self) -> VisualTokenContract {
        VisualTokenContract {
            background: Color::Surface,
            foreground: self.text_color(),
            padding: self.padding(),
            radius: self.corner_radius(),
            metric: self.metric(),
            motion: Motion::Fast,
            typography: self.text_style(),
        }
    }
}

/// Normalized keyboard input consumed by SolUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Open the standard command palette (`Ctrl+Shift+P` on desktop keyboards).
    CommandPalette,
    /// Advance focus to the next focusable semantic control.
    Tab,
    /// Move focus to the previous focusable semantic control.
    ShiftTab,
    /// Activate the focused control.
    Enter,
    /// Activate the focused control.
    Space,
    /// Select the previous tab when a tab is focused.
    ArrowLeft,
    /// Select the next tab when a tab is focused.
    ArrowRight,
    /// Insert one text character into the focused editable field.
    Character(char),
    /// Remove one Unicode scalar from the focused editable field.
    Backspace,
    /// Dismiss the current transient surface without activating a control.
    Escape,
}

/// Uniform outcome returned by keyboard dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyboardOutcome {
    /// The key did not apply to the currently focused control.
    Ignored,
    /// Keyboard focus moved to the given semantic control.
    FocusMoved(SemanticId),
    /// An activatable control was triggered.
    Activated(SemanticId),
    /// A tab was selected.
    SelectionChanged(SemanticId),
    /// Text content was edited.
    TextChanged(SemanticId),
}

/// A semantic control retained by the interaction tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticControl {
    /// An activatable button.
    Button {
        /// Stable ID.
        id: SemanticId,
        /// Accessible label.
        label: String,
        /// Whether keyboard activation is allowed.
        enabled: bool,
    },
    /// A text field with retained content.
    TextField {
        /// Stable ID.
        id: SemanticId,
        /// Accessible label.
        label: String,
        /// Current retained text.
        value: String,
        /// Whether this field accepts edits.
        editable: bool,
    },
    /// A selectable tab.
    Tab {
        /// Stable ID.
        id: SemanticId,
        /// Accessible label.
        label: String,
        /// Whether keyboard selection is allowed.
        enabled: bool,
        /// Current selection state.
        selected: bool,
    },
}

impl SemanticControl {
    /// Build a semantic button from an existing SolUI component.
    pub fn button(id: impl Into<String>, button: &Button) -> Self {
        Self::Button {
            id: SemanticId::new(id),
            label: button.label.to_owned(),
            enabled: button.enabled,
        }
    }

    /// Build a semantic text field from an existing SolUI component.
    pub fn text_field(id: impl Into<String>, field: &TextField) -> Self {
        Self::TextField {
            id: SemanticId::new(id),
            label: field.placeholder.to_owned(),
            value: field.text.clone(),
            editable: field.editable,
        }
    }

    /// Build a semantic tab from an existing SolUI component.
    pub fn tab(id: impl Into<String>, tab: &Tab) -> Self {
        Self::Tab {
            id: SemanticId::new(id),
            label: tab.label.to_owned(),
            enabled: tab.enabled,
            selected: tab.selected,
        }
    }

    fn id(&self) -> &SemanticId {
        match self {
            Self::Button { id, .. } | Self::TextField { id, .. } | Self::Tab { id, .. } => id,
        }
    }

    fn is_focusable(&self) -> bool {
        match self {
            Self::Button { enabled, .. } | Self::Tab { enabled, .. } => *enabled,
            Self::TextField { .. } => true,
        }
    }

    fn accessibility_node(&self, focused: bool) -> AccessibilityNode {
        match self {
            Self::Button { id, label, enabled } => AccessibilityNode {
                id: id.clone(),
                role: SemanticRole::Button,
                label: label.clone(),
                value: None,
                state: AccessibilityState {
                    focused,
                    disabled: !enabled,
                    ..AccessibilityState::default()
                },
                children: Vec::new(),
            },
            Self::TextField {
                id,
                label,
                value,
                editable,
            } => AccessibilityNode {
                id: id.clone(),
                role: SemanticRole::TextField,
                label: label.clone(),
                value: Some(value.clone()),
                state: AccessibilityState {
                    focused,
                    editable: *editable,
                    ..AccessibilityState::default()
                },
                children: Vec::new(),
            },
            Self::Tab {
                id,
                label,
                enabled,
                selected,
            } => AccessibilityNode {
                id: id.clone(),
                role: SemanticRole::Tab,
                label: label.clone(),
                value: None,
                state: AccessibilityState {
                    focused,
                    disabled: !enabled,
                    selected: *selected,
                    ..AccessibilityState::default()
                },
                children: Vec::new(),
            },
        }
    }
}

/// Retained focus order, keyboard behavior, and accessibility tree for one
/// semantic view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractionTree {
    root: SemanticId,
    label: String,
    controls: Vec<SemanticControl>,
    focused: Option<usize>,
}

impl InteractionTree {
    /// Create an empty semantic group.
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            root: SemanticId::new(id),
            label: label.into(),
            controls: Vec::new(),
            focused: None,
        }
    }

    /// Append a semantic control in focus traversal order.
    pub fn push(&mut self, control: SemanticControl) {
        self.controls.push(control);
    }

    /// Focus a particular control by stable semantic ID.
    pub fn focus(&mut self, id: &str) -> Option<SemanticId> {
        let index = self
            .controls
            .iter()
            .position(|control| control.id().as_str() == id && control.is_focusable())?;
        self.focused = Some(index);
        Some(self.controls[index].id().clone())
    }

    /// Return the focused semantic ID, if a focusable control has one.
    pub fn focused(&self) -> Option<&SemanticId> {
        self.focused.map(|index| self.controls[index].id())
    }

    /// Dispatch one normalized keyboard key according to SolUI behavior.
    pub fn handle_key(&mut self, key: Key) -> KeyboardOutcome {
        match key {
            Key::CommandPalette | Key::Escape => KeyboardOutcome::Ignored,
            Key::Tab => self.move_focus(false),
            Key::ShiftTab => self.move_focus(true),
            Key::Enter | Key::Space => self.activate_focused(),
            Key::ArrowLeft => self.select_adjacent_tab(true),
            Key::ArrowRight => self.select_adjacent_tab(false),
            Key::Character(character) => self.edit_text(Some(character)),
            Key::Backspace => self.edit_text(None),
        }
    }

    /// Build the current renderer-neutral semantic accessibility tree.
    pub fn accessibility_tree(&self) -> AccessibilityNode {
        AccessibilityNode {
            id: self.root.clone(),
            role: SemanticRole::Group,
            label: self.label.clone(),
            value: None,
            state: AccessibilityState::default(),
            children: self
                .controls
                .iter()
                .enumerate()
                .map(|(index, control)| control.accessibility_node(self.focused == Some(index)))
                .collect(),
        }
    }

    fn move_focus(&mut self, reverse: bool) -> KeyboardOutcome {
        let len = self.controls.len();
        if len == 0 {
            return KeyboardOutcome::Ignored;
        }
        let start = self.focused.unwrap_or(if reverse { 0 } else { len - 1 });
        for offset in 1..=len {
            let index = if reverse {
                (start + len - (offset % len)) % len
            } else {
                (start + offset) % len
            };
            if self.controls[index].is_focusable() {
                self.focused = Some(index);
                return KeyboardOutcome::FocusMoved(self.controls[index].id().clone());
            }
        }
        KeyboardOutcome::Ignored
    }

    fn activate_focused(&mut self) -> KeyboardOutcome {
        let Some(index) = self.focused else {
            return KeyboardOutcome::Ignored;
        };
        match &self.controls[index] {
            SemanticControl::Button { id, enabled, .. } if *enabled => {
                KeyboardOutcome::Activated(id.clone())
            }
            SemanticControl::Tab { .. } => self.select_tab(index),
            _ => KeyboardOutcome::Ignored,
        }
    }

    fn select_adjacent_tab(&mut self, reverse: bool) -> KeyboardOutcome {
        let Some(current) = self.focused else {
            return KeyboardOutcome::Ignored;
        };
        if !matches!(self.controls[current], SemanticControl::Tab { .. }) {
            return KeyboardOutcome::Ignored;
        }
        let tab_indices: Vec<usize> = self
            .controls
            .iter()
            .enumerate()
            .filter_map(|(index, control)| {
                matches!(control, SemanticControl::Tab { enabled: true, .. }).then_some(index)
            })
            .collect();
        let Some(position) = tab_indices.iter().position(|index| *index == current) else {
            return KeyboardOutcome::Ignored;
        };
        let next = if reverse {
            (position + tab_indices.len() - 1) % tab_indices.len()
        } else {
            (position + 1) % tab_indices.len()
        };
        let index = tab_indices[next];
        self.focused = Some(index);
        self.select_tab(index)
    }

    fn select_tab(&mut self, index: usize) -> KeyboardOutcome {
        let selected_id = self.controls[index].id().clone();
        if !matches!(
            self.controls[index],
            SemanticControl::Tab { enabled: true, .. }
        ) {
            return KeyboardOutcome::Ignored;
        }
        for control in &mut self.controls {
            if let SemanticControl::Tab { id, selected, .. } = control {
                *selected = id == &selected_id;
            }
        }
        KeyboardOutcome::SelectionChanged(selected_id)
    }

    fn edit_text(&mut self, character: Option<char>) -> KeyboardOutcome {
        let Some(index) = self.focused else {
            return KeyboardOutcome::Ignored;
        };
        let SemanticControl::TextField {
            id,
            value,
            editable,
            ..
        } = &mut self.controls[index]
        else {
            return KeyboardOutcome::Ignored;
        };
        if !*editable {
            return KeyboardOutcome::Ignored;
        }
        if let Some(character) = character {
            value.push(character);
        } else {
            let _ = value.pop();
        }
        KeyboardOutcome::TextChanged(id.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interaction_tree() -> InteractionTree {
        let mut tree = InteractionTree::new("window", "Settings");
        tree.push(SemanticControl::button(
            "open",
            &Button::new().with_label("Open"),
        ));
        tree.push(SemanticControl::button(
            "disabled",
            &Button::new().with_label("Disabled").enabled(false),
        ));
        tree.push(SemanticControl::text_field(
            "search",
            &TextField::new().with_placeholder("Search"),
        ));
        tree.push(SemanticControl::tab(
            "general",
            &Tab::new("General").select(),
        ));
        tree.push(SemanticControl::tab("advanced", &Tab::new("Advanced")));
        tree
    }

    #[test]
    fn traversal_skips_disabled_controls_and_wraps() {
        let mut tree = interaction_tree();
        assert_eq!(
            tree.handle_key(Key::Tab),
            KeyboardOutcome::FocusMoved(SemanticId::new("open"))
        );
        assert_eq!(
            tree.handle_key(Key::Tab),
            KeyboardOutcome::FocusMoved(SemanticId::new("search"))
        );
        assert_eq!(
            tree.handle_key(Key::ShiftTab),
            KeyboardOutcome::FocusMoved(SemanticId::new("open"))
        );
    }

    #[test]
    fn keyboard_activates_buttons_selects_tabs_and_edits_text() {
        let mut tree = interaction_tree();
        tree.focus("open");
        assert_eq!(
            tree.handle_key(Key::Enter),
            KeyboardOutcome::Activated(SemanticId::new("open"))
        );

        tree.focus("advanced");
        assert_eq!(
            tree.handle_key(Key::ArrowLeft),
            KeyboardOutcome::SelectionChanged(SemanticId::new("general"))
        );
        let selected = tree.accessibility_tree();
        assert!(selected.children[3].state.selected);
        assert!(!selected.children[4].state.selected);

        tree.focus("search");
        assert_eq!(
            tree.handle_key(Key::Character('S')),
            KeyboardOutcome::TextChanged(SemanticId::new("search"))
        );
        assert_eq!(
            tree.accessibility_tree().children[2].value.as_deref(),
            Some("S")
        );
        assert_eq!(
            tree.handle_key(Key::Backspace),
            KeyboardOutcome::TextChanged(SemanticId::new("search"))
        );
        assert_eq!(
            tree.accessibility_tree().children[2].value.as_deref(),
            Some("")
        );
    }

    #[test]
    fn semantic_tree_exposes_focus_selection_and_editability() {
        let mut tree = interaction_tree();
        tree.focus("search");
        let root = tree.accessibility_tree();
        assert_eq!(root.role, SemanticRole::Group);
        assert!(root.children[2].state.focused);
        assert!(root.children[2].state.editable);
        assert!(root.children[3].state.selected);
    }

    #[test]
    fn visual_contract_snapshot_contains_only_token_names() {
        let snapshot = Button::new().with_label("Open").visual_tokens().snapshot();
        assert_eq!(
            snapshot,
            "background=Elevated;foreground=TextPrimary;padding=Md;radius=Sm;metric=Button;motion=Fast;typography=Label"
        );
    }
}
