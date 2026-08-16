//! Renderer-neutral command-palette behavior shared by first-party SOL apps.
//!
//! The palette owns only keyboard navigation, filtering, and semantic
//! accessibility projection. Applications own command effects and call their
//! typed execution APIs after receiving [`CommandPaletteOutcome::Execute`].

use crate::{AccessibilityNode, AccessibilityState, Key, SemanticId, SemanticRole};

/// Desktop shortcut normalized by native input adapters to [`Key::CommandPalette`].
pub const COMMAND_PALETTE_SHORTCUT: &str = "Ctrl+Shift+P";

/// Stable metadata for an application command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteCommand {
    /// Stable application-owned command identifier.
    pub id: &'static str,
    /// User-visible command title.
    pub title: &'static str,
}

/// State exposed by a command palette without requiring a concrete renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandPaletteState {
    /// The transient palette is not visible.
    Closed,
    /// The palette is visible and has one or more matching commands.
    Open,
    /// The palette is visible but the current query has no matching commands.
    Empty,
}

/// Result of one standard command-palette key event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandPaletteOutcome {
    /// The key does not apply to the palette in its current state.
    Ignored,
    /// The standard shortcut made the palette visible with search focus.
    Opened,
    /// Escape dismissed the transient palette without executing a command.
    Closed,
    /// Keyboard focus moved within the palette.
    FocusMoved(SemanticId),
    /// The search query changed and the matching command list was recalculated.
    QueryChanged,
    /// Enter or Space activated the selected command. The app must execute it.
    Execute(&'static str),
}

/// Retained command-palette interaction model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPalette {
    commands: &'static [PaletteCommand],
    open: bool,
    query: String,
    focused: usize,
}

impl CommandPalette {
    /// Create a closed palette over a stable app-owned command catalog.
    #[must_use]
    pub const fn new(commands: &'static [PaletteCommand]) -> Self {
        Self {
            commands,
            open: false,
            query: String::new(),
            focused: 0,
        }
    }

    /// Return the uniformly filtered command list for `query`.
    #[must_use]
    pub fn filter(commands: &'static [PaletteCommand], query: &str) -> Vec<PaletteCommand> {
        let query = query.to_ascii_lowercase();
        commands
            .iter()
            .copied()
            .filter(|command| {
                query.is_empty()
                    || command.id.contains(&query)
                    || command.title.to_ascii_lowercase().contains(&query)
            })
            .collect()
    }

    /// Return whether the palette is closed, open, or empty.
    #[must_use]
    pub fn state(&self) -> CommandPaletteState {
        if !self.open {
            CommandPaletteState::Closed
        } else if self.matches().is_empty() {
            CommandPaletteState::Empty
        } else {
            CommandPaletteState::Open
        }
    }

    /// Return the retained search query.
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Return commands matching the retained query in catalog order.
    #[must_use]
    pub fn matches(&self) -> Vec<PaletteCommand> {
        Self::filter(self.commands, &self.query)
    }

    /// Handle the common shortcut, traversal, activation, editing, and escape keys.
    pub fn handle_key(&mut self, key: Key) -> CommandPaletteOutcome {
        if matches!(key, Key::CommandPalette) {
            if !self.open {
                self.query.clear();
            }
            self.open = true;
            self.focused = 0;
            return CommandPaletteOutcome::Opened;
        }
        if !self.open {
            return CommandPaletteOutcome::Ignored;
        }
        match key {
            Key::Escape => {
                self.open = false;
                self.focused = 0;
                CommandPaletteOutcome::Closed
            }
            Key::Tab => self.move_focus(false),
            Key::ShiftTab => self.move_focus(true),
            Key::Character(character) if self.focused == 0 => {
                self.query.push(character);
                self.clamp_focus();
                CommandPaletteOutcome::QueryChanged
            }
            Key::Backspace if self.focused == 0 => {
                if self.query.pop().is_some() {
                    self.clamp_focus();
                    CommandPaletteOutcome::QueryChanged
                } else {
                    CommandPaletteOutcome::Ignored
                }
            }
            Key::Enter | Key::Space => self.activate(),
            Key::CommandPalette
            | Key::ArrowLeft
            | Key::ArrowRight
            | Key::Character(_)
            | Key::Backspace => CommandPaletteOutcome::Ignored,
        }
    }

    /// Build the current semantic projection for an accessibility bridge.
    #[must_use]
    pub fn accessibility_tree(&self) -> AccessibilityNode {
        if !self.open {
            return AccessibilityNode {
                id: SemanticId::new("command-palette"),
                role: SemanticRole::Group,
                label: "Command palette".to_owned(),
                value: Some(COMMAND_PALETTE_SHORTCUT.to_owned()),
                state: AccessibilityState::default(),
                children: Vec::new(),
            };
        }
        let mut children = vec![AccessibilityNode {
            id: SemanticId::new("command-palette.search"),
            role: SemanticRole::TextField,
            label: "Search commands".to_owned(),
            value: Some(self.query.clone()),
            state: AccessibilityState {
                focused: self.open && self.focused == 0,
                editable: self.open,
                ..AccessibilityState::default()
            },
            children: Vec::new(),
        }];
        let matches = self.matches();
        if self.open && matches.is_empty() {
            children.push(AccessibilityNode {
                id: SemanticId::new("command-palette.empty"),
                role: SemanticRole::Group,
                label: "No matching commands".to_owned(),
                value: None,
                state: AccessibilityState::default(),
                children: Vec::new(),
            });
        } else {
            children.extend(
                matches
                    .iter()
                    .enumerate()
                    .map(|(index, command)| AccessibilityNode {
                        id: command_id(command.id),
                        role: SemanticRole::Button,
                        label: command.title.to_owned(),
                        value: Some(command.id.to_owned()),
                        state: AccessibilityState {
                            focused: self.open && self.focused == index + 1,
                            disabled: !self.open,
                            ..AccessibilityState::default()
                        },
                        children: Vec::new(),
                    }),
            );
        }
        AccessibilityNode {
            id: SemanticId::new("command-palette"),
            role: SemanticRole::Group,
            label: "Command palette".to_owned(),
            value: Some(COMMAND_PALETTE_SHORTCUT.to_owned()),
            state: AccessibilityState::default(),
            children,
        }
    }

    fn move_focus(&mut self, reverse: bool) -> CommandPaletteOutcome {
        let len = self.matches().len() + 1;
        self.focused = if reverse {
            (self.focused + len - 1) % len
        } else {
            (self.focused + 1) % len
        };
        CommandPaletteOutcome::FocusMoved(self.focused_id())
    }

    fn activate(&self) -> CommandPaletteOutcome {
        if self.focused == 0 {
            return CommandPaletteOutcome::Ignored;
        }
        self.matches()
            .get(self.focused - 1)
            .map(|command| CommandPaletteOutcome::Execute(command.id))
            .unwrap_or(CommandPaletteOutcome::Ignored)
    }

    fn clamp_focus(&mut self) {
        self.focused = self.focused.min(self.matches().len());
    }

    fn focused_id(&self) -> SemanticId {
        if self.focused == 0 {
            SemanticId::new("command-palette.search")
        } else {
            self.matches()
                .get(self.focused - 1)
                .map(|command| command_id(command.id))
                .unwrap_or_else(|| SemanticId::new("command-palette.search"))
        }
    }
}

fn command_id(id: &str) -> SemanticId {
    SemanticId::new(format!("command-palette.command.{id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SETTINGS: [PaletteCommand; 2] = [
        PaletteCommand {
            id: "appearance.dark",
            title: "Use dark appearance",
        },
        PaletteCommand {
            id: "sound.mute",
            title: "Toggle output mute",
        },
    ];
    const TERMINAL: [PaletteCommand; 2] = [
        PaletteCommand {
            id: "terminal.new_tab",
            title: "Open a new terminal tab",
        },
        PaletteCommand {
            id: "terminal.copy",
            title: "Copy terminal selection",
        },
    ];
    const FILES: [PaletteCommand; 2] = [
        PaletteCommand {
            id: "view.grid",
            title: "Use grid view",
        },
        PaletteCommand {
            id: "directory.refresh",
            title: "Refresh folder",
        },
    ];

    #[test]
    fn dogfood_catalogs_share_shortcut_search_focus_activation_escape_and_empty_contract() {
        for commands in [&SETTINGS[..], &TERMINAL[..], &FILES[..]] {
            let mut palette = CommandPalette::new(commands);
            assert_eq!(
                palette.handle_key(Key::CommandPalette),
                CommandPaletteOutcome::Opened
            );
            assert!(matches!(
                palette.handle_key(Key::Tab),
                CommandPaletteOutcome::FocusMoved(_)
            ));
            assert!(matches!(
                palette.handle_key(Key::Enter),
                CommandPaletteOutcome::Execute(_)
            ));
            assert_eq!(
                palette.handle_key(Key::Escape),
                CommandPaletteOutcome::Closed
            );
            assert!(palette.accessibility_tree().children.is_empty());

            palette.handle_key(Key::CommandPalette);
            palette.handle_key(Key::Character('z'));
            assert_eq!(palette.state(), CommandPaletteState::Empty);
            assert_eq!(
                palette.accessibility_tree().children[1].label,
                "No matching commands"
            );
        }
    }

    #[test]
    fn shift_tab_wraps_and_query_is_editable_only_at_search_focus() {
        let mut palette = CommandPalette::new(&SETTINGS);
        palette.handle_key(Key::CommandPalette);
        assert!(matches!(
            palette.handle_key(Key::ShiftTab),
            CommandPaletteOutcome::FocusMoved(_)
        ));
        assert_eq!(
            palette.handle_key(Key::Character('k')),
            CommandPaletteOutcome::Ignored
        );
        palette.handle_key(Key::Tab);
        assert_eq!(
            palette.handle_key(Key::Character('k')),
            CommandPaletteOutcome::QueryChanged
        );
        assert_eq!(palette.matches(), vec![SETTINGS[0]]);
    }
}
