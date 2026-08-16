//! sol-ui — Semantic UI components for SolKit
//!
//! This crate provides SOL-native UI components that use `sol-design` tokens
//! for visual consistency. Per ADR-0004, Slint is the intended rendering
//! substrate, but the semantic API is independent of the rendering backend.
//!
//! # Layering
//!
//! ```text
//! Application code → Semantic components (Button, TextField, etc.)
//!                               ↓
//!                sol-design tokens (colors, spacing, motion)
//!                               ↓
//!                Slint rendering substrate (pending spike validation)
//! ```
//!
//! # Motivation
//!
//! PRD §19.1: "Consistency First" — components use tokens, not hand-written
//! values. This ensures visual consistency across Shell and first-party apps.

use sol_design::{color::Color, motion::Motion, radius::Radius, spacing::Spacing};

/// A semantic button component.
///
/// Uses `Color` and `Spacing` tokens from sol-design for styling.
/// Applications write intent ("this is a button") not visual metrics.
///
/// # Example
///
/// ```
/// use sol_ui::{Button, ButtonState};
///
/// let button = Button::new()
///     .with_label("Click me")
///     .enabled(true);
/// ```
#[derive(Debug)]
pub struct Button {
    /// The button's intrinsic size (adjusted by layout).
    pub size: (f32, f32),
    /// Whether the button is enabled.
    pub enabled: bool,
    /// Whether the button is pressed/hovered for visual feedback.
    pub state: ButtonState,
    /// The label text.
    pub label: &'static str,
}

/// Visual state of a button for consistent feedback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonState {
    /// Normal, ready for interaction.
    #[default]
    Normal,
    /// Pointer is over the button.
    Hovered,
    /// Button is being pressed.
    Pressed,
    /// Disabled (non-interactive).
    Disabled,
}

impl Default for Button {
    fn default() -> Self {
        Self {
            size: (100.0, 32.0),
            enabled: true,
            state: ButtonState::Normal,
            label: "",
        }
    }
}

impl Button {
    /// Create a new button with default styling.
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure the label text for the button.
    pub fn with_label(mut self, label: &'static str) -> Self {
        self.label = label;
        self
    }

    /// Enable or disable the button.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        if !enabled {
            self.state = ButtonState::Disabled;
        }
        self
    }

    /// Set the button state.
    pub fn state(mut self, state: ButtonState) -> Self {
        self.state = state;
        self
    }

    /// Get the background color for the current state.
    /// Uses sol-design Color tokens.
    pub fn background(&self) -> Color {
        if !self.enabled {
            return Color::Surface;
        }
        match self.state {
            ButtonState::Pressed => Color::Accent,
            ButtonState::Hovered => Color::Elevated,
            ButtonState::Normal => Color::Elevated,
            ButtonState::Disabled => Color::Surface,
        }
    }

    /// Get the corner radius for the button.
    pub fn corner_radius(&self) -> Radius {
        Radius::Sm
    }

    /// Get the horizontal padding inside the button.
    pub fn padding_x(&self) -> Spacing {
        Spacing::Md
    }

    /// Get the vertical padding inside the button.
    pub fn padding_y(&self) -> Spacing {
        Spacing::Sm
    }

    /// Get the animation spec for state transitions.
    pub fn motion(&self) -> Motion {
        Motion::Fast
    }
}

/// A horizontal stack layout container.
///
/// Provides the basic layout primitive used by SolKit apps.
pub struct HStack {
    /// Spacing between children.
    pub spacing: Spacing,
    /// Alignment of children along the main axis.
    pub alignment: StackAlignment,
    /// Children elements.
    pub children: Vec<StackItem>,
}

impl Default for HStack {
    fn default() -> Self {
        Self {
            spacing: Spacing::default(),
            alignment: StackAlignment::Start,
            children: Vec::new(),
        }
    }
}

/// Alignment options for stack layouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StackAlignment {
    /// Start/cross-start edge.
    #[default]
    Start,
    /// Center.
    Center,
    /// End/cross-end edge.
    End,
    /// Stretch to fill available space.
    Stretch,
}

/// An item that can be placed in a stack layout.
#[derive(Debug)]
pub enum StackItem {
    /// A button widget.
    Button(Button),
    /// A spacer that expands.
    Spacer,
}

impl HStack {
    /// Create a new horizontal stack with default spacing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the spacing between items.
    pub fn spacing(mut self, spacing: Spacing) -> Self {
        self.spacing = spacing;
        self
    }

    /// Set the alignment of children.
    pub fn alignment(mut self, alignment: StackAlignment) -> Self {
        self.alignment = alignment;
        self
    }

    /// Add an item to the stack.
    pub fn item(mut self, item: StackItem) -> Self {
        self.children.push(item);
        self
    }
}

/// A vertical stack layout container.
///
/// Like HStack but with vertical main axis.
pub struct VStack {
    /// Spacing between children.
    pub spacing: Spacing,
    /// Alignment of children.
    pub alignment: StackAlignment,
    /// Children elements.
    pub children: Vec<StackItem>,
}

impl Default for VStack {
    fn default() -> Self {
        Self {
            spacing: Spacing::default(),
            alignment: StackAlignment::Start,
            children: Vec::new(),
        }
    }
}

impl VStack {
    /// Create a new vertical stack with default spacing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the spacing between items.
    pub fn spacing(mut self, spacing: Spacing) -> Self {
        self.spacing = spacing;
        self
    }

    /// Set the alignment of children.
    pub fn alignment(mut self, alignment: StackAlignment) -> Self {
        self.alignment = alignment;
        self
    }

    /// Add an item to the stack.
    pub fn item(mut self, item: StackItem) -> Self {
        self.children.push(item);
        self
    }
}

/// A text field component.
#[derive(Debug)]
pub struct TextField {
    /// The text content.
    pub text: String,
    /// Placeholder text shown when empty.
    pub placeholder: &'static str,
    /// Whether the field is editable.
    pub editable: bool,
}

impl Default for TextField {
    fn default() -> Self {
        Self {
            text: String::new(),
            placeholder: "",
            editable: true,
        }
    }
}

impl TextField {
    /// Create a new text field.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the placeholder text.
    pub fn with_placeholder(mut self, placeholder: &'static str) -> Self {
        self.placeholder = placeholder;
        self
    }

    /// Set whether the field is editable.
    pub fn editable(mut self, editable: bool) -> Self {
        self.editable = editable;
        self
    }

    /// Get the text color for the current text.
    pub fn text_color(&self) -> Color {
        Color::TextPrimary
    }

    /// Get the placeholder color.
    pub fn placeholder_color(&self) -> Color {
        Color::TextSecondary
    }

    /// Get the padding inside the field.
    pub fn padding(&self) -> Spacing {
        Spacing::Md
    }

    /// Get the corner radius.
    pub fn corner_radius(&self) -> Radius {
        Radius::Sm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_default_is_enabled() {
        let button = Button::new();
        assert!(button.enabled);
        assert!(matches!(button.state, ButtonState::Normal));
    }

    #[test]
    fn button_disabled_state_uses_surface_color() {
        let button = Button::new().enabled(false);
        let bg = button.background();
        assert!(matches!(bg, Color::Surface));
    }

    #[test]
    fn button_hover_state_uses_elevated_color() {
        let mut button = Button::new();
        button.state = ButtonState::Hovered;
        let bg = button.background();
        assert!(matches!(bg, Color::Elevated));
    }

    #[test]
    fn button_pressed_state_uses_accent_color() {
        let mut button = Button::new();
        button.state = ButtonState::Pressed;
        let bg = button.background();
        assert!(matches!(bg, Color::Accent));
    }

    #[test]
    fn hstack_default_spacing_is_md() {
        let stack = HStack::new();
        assert_eq!(stack.spacing, Spacing::Md);
    }

    #[test]
    fn hstack_alignment_can_be_configured() {
        let stack = HStack::new().alignment(StackAlignment::Center);
        assert!(matches!(stack.alignment, StackAlignment::Center));
    }

    #[test]
    fn hstack_can_add_children() {
        let stack = HStack::new()
            .item(StackItem::Button(Button::new()))
            .item(StackItem::Spacer);
        assert_eq!(stack.children.len(), 2);
    }

    #[test]
    fn textfield_default_is_editable() {
        let field = TextField::new();
        assert!(field.editable);
    }
}

/// A toolbar component - horizontal container for actions.
#[derive(Debug)]
pub struct Toolbar {
    /// Items in the toolbar.
    pub items: Vec<ToolbarItem>,
    /// The background color.
    pub background: Color,
}

impl Default for Toolbar {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            background: Color::Elevated,
        }
    }
}

impl Toolbar {
    /// Create a new toolbar.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an item to the toolbar.
    pub fn item(mut self, item: ToolbarItem) -> Self {
        self.items.push(item);
        self
    }

    /// Get the toolbar background color.
    pub fn background_color(&self) -> Color {
        self.background
    }

    /// Get the height of the toolbar.
    pub fn height(&self) -> f32 {
        40.0
    }

    /// Get the item spacing.
    pub fn item_spacing(&self) -> Spacing {
        Spacing::Sm
    }
}

/// An item in a toolbar.
#[derive(Debug)]
pub enum ToolbarItem {
    /// A button.
    Button(Button),
    /// A separator.
    Separator,
    /// A label.
    Label(&'static str),
}

/// A tab component.
#[derive(Debug)]
pub struct Tab {
    /// The tab label.
    pub label: &'static str,
    /// Whether this tab is selected.
    pub selected: bool,
    /// Whether this tab is enabled.
    pub enabled: bool,
}

impl Tab {
    /// Create a new tab.
    pub fn new(label: &'static str) -> Self {
        Self {
            label,
            selected: false,
            enabled: true,
        }
    }

    /// Select this tab.
    pub fn select(mut self) -> Self {
        self.selected = true;
        self
    }

    /// Disable this tab.
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// Get the text color based on state.
    pub fn text_color(&self) -> Color {
        if !self.enabled {
            Color::TextSecondary
        } else if self.selected {
            Color::Accent
        } else {
            Color::TextPrimary
        }
    }

    /// Get the indicator color for selected tab.
    pub fn indicator_color(&self) -> Color {
        Color::Accent
    }
}

/// A tab bar container.
#[derive(Debug, Default)]
pub struct TabBar {
    /// The tabs in the bar.
    pub tabs: Vec<Tab>,
}

impl TabBar {
    /// Create a new tab bar.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a tab to the bar.
    pub fn tab(mut self, tab: Tab) -> Self {
        self.tabs.push(tab);
        self
    }

    /// Get the tab height.
    pub fn tab_height(&self) -> f32 {
        32.0
    }
}

#[cfg(test)]
mod ui_component_tests {
    use super::*;

    #[test]
    fn toolbar_default_background_is_elevated() {
        let toolbar = Toolbar::new();
        assert!(matches!(toolbar.background, Color::Elevated));
    }

    #[test]
    fn toolbar_can_add_items() {
        let toolbar = Toolbar::new()
            .item(ToolbarItem::Button(Button::new()))
            .item(ToolbarItem::Separator)
            .item(ToolbarItem::Label("File"));
        assert_eq!(toolbar.items.len(), 3);
    }

    #[test]
    fn tab_selected_uses_accent_color() {
        let tab = Tab::new("Tests").select();
        assert!(tab.selected);
        assert!(matches!(tab.text_color(), Color::Accent));
    }

    #[test]
    fn tab_disabled_uses_secondary_color() {
        let tab = Tab::new("Tests").disabled();
        assert!(!tab.enabled);
        assert!(matches!(tab.text_color(), Color::TextSecondary));
    }

    #[test]
    fn tabbar_can_add_tabs() {
        let tabbar = TabBar::new()
            .tab(Tab::new("General"))
            .tab(Tab::new("Tests"));
        assert_eq!(tabbar.tabs.len(), 2);
    }
}
