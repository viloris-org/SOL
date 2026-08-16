//! First-party Settings application core. The UI state is renderer-neutral and
//! talks only to the typed `SettingsApi` boundary.

use sol_app::{App, AppId};
use sol_design::accessibility::{TextScale as UiTextScale, TokenMode};
use sol_system::{
    ColorScheme, OutputVolume, SettingsApi, SettingsChange, SettingsError, SettingsSnapshot,
    TextScale,
};
use sol_ui::{
    AccessibilityNode, Button, CommandPalette, CommandPaletteOutcome, InteractionTree, Key,
    KeyboardOutcome, PaletteCommand, SemanticControl,
};

const SETTINGS_COMMANDS: [PaletteCommand; 7] = [
    PaletteCommand {
        id: "appearance.dark",
        title: "Use dark appearance",
    },
    PaletteCommand {
        id: "appearance.high_contrast",
        title: "Toggle high contrast",
    },
    PaletteCommand {
        id: "appearance.reduced_motion",
        title: "Toggle reduced motion",
    },
    PaletteCommand {
        id: "appearance.text_large",
        title: "Use large text",
    },
    PaletteCommand {
        id: "sound.mute",
        title: "Toggle output mute",
    },
    PaletteCommand {
        id: "page.displays",
        title: "Open Displays",
    },
    PaletteCommand {
        id: "page.keyboard",
        title: "Open Keyboard",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Appearance,
    Sound,
    Displays,
    Keyboard,
}

impl Page {
    fn title(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Sound => "Sound",
            Self::Displays => "Displays",
            Self::Keyboard => "Keyboard",
        }
    }
}

/// Shared command-palette metadata for Settings actions.
pub type CommandItem = PaletteCommand;

pub struct SettingsApp<A: SettingsApi> {
    api: A,
    pub app: App,
    snapshot: SettingsSnapshot,
    page: Page,
    tree: InteractionTree,
    palette: CommandPalette,
}

impl<A: SettingsApi> SettingsApp<A> {
    pub fn new(api: A) -> Result<Self, SettingsError> {
        let id = AppId::parse("org.sol.settings")
            .map_err(|error| SettingsError::backend(error.to_string()))?;
        let snapshot = api.snapshot()?;
        let mut tree = InteractionTree::new("settings", "Settings");
        tree.push(SemanticControl::button(
            "appearance",
            &Button::new().with_label("Appearance"),
        ));
        tree.push(SemanticControl::button(
            "sound",
            &Button::new().with_label("Sound"),
        ));
        tree.push(SemanticControl::button(
            "displays",
            &Button::new().with_label("Displays"),
        ));
        tree.push(SemanticControl::button(
            "keyboard",
            &Button::new().with_label("Keyboard"),
        ));
        Ok(Self {
            api,
            app: App::new(id),
            snapshot,
            page: Page::Appearance,
            tree,
            palette: CommandPalette::new(&SETTINGS_COMMANDS),
        })
    }
    pub fn snapshot(&self) -> &SettingsSnapshot {
        &self.snapshot
    }
    pub fn page(&self) -> Page {
        self.page
    }
    pub fn token_mode(&self) -> TokenMode {
        let appearance = &self.snapshot.appearance;
        let mut mode = match appearance.color_scheme {
            ColorScheme::Dark => TokenMode::dark(),
            ColorScheme::Light | ColorScheme::System => TokenMode::light(),
        };
        if appearance.high_contrast {
            mode = mode.high_contrast();
        }
        if appearance.reduced_motion {
            mode = mode.reduced_motion();
        }
        mode.with_text_scale(match appearance.text_scale {
            TextScale::Default => UiTextScale::Default,
            TextScale::Large => UiTextScale::Large,
            TextScale::ExtraLarge => UiTextScale::ExtraLarge,
        })
    }
    pub fn commands(query: &str) -> Vec<CommandItem> {
        CommandPalette::filter(&SETTINGS_COMMANDS, query)
    }
    pub fn execute(&mut self, id: &str) -> Result<(), SettingsError> {
        match id {
            "appearance.dark" => self.apply(SettingsChange::SetColorScheme(ColorScheme::Dark)),
            "appearance.high_contrast" => self.apply(SettingsChange::SetHighContrast(
                !self.snapshot.appearance.high_contrast,
            )),
            "appearance.reduced_motion" => self.apply(SettingsChange::SetReducedMotion(
                !self.snapshot.appearance.reduced_motion,
            )),
            "appearance.text_large" => self.apply(SettingsChange::SetTextScale(TextScale::Large)),
            "sound.mute" => self.apply(SettingsChange::SetOutputMuted(
                !self.snapshot.audio.output_muted,
            )),
            "page.displays" => {
                self.page = Page::Displays;
                Ok(())
            }
            "page.keyboard" => {
                self.page = Page::Keyboard;
                Ok(())
            }
            _ => Err(SettingsError::backend(format!(
                "unknown settings command: {id}"
            ))),
        }
    }
    pub fn set_volume(&mut self, percent: u8) -> Result<(), SettingsError> {
        self.apply(SettingsChange::SetOutputVolume(OutputVolume::new(percent)?))
    }
    pub fn unavailable_message(&self) -> Option<&'static str> {
        match self.page {
            Page::Displays => Some(
                "Display configuration is unavailable until a typed display service API is provided.",
            ),
            Page::Keyboard => Some(
                "Keyboard and input configuration is unavailable until a typed input service API is provided.",
            ),
            _ => None,
        }
    }
    pub fn handle_key(&mut self, key: Key) -> KeyboardOutcome {
        self.tree.handle_key(key)
    }
    /// Route the shared command-palette key contract and execute activated commands.
    pub fn handle_command_palette_key(
        &mut self,
        key: Key,
    ) -> Result<CommandPaletteOutcome, SettingsError> {
        let outcome = self.palette.handle_key(key);
        if let CommandPaletteOutcome::Execute(id) = outcome {
            self.execute(id)?;
            Ok(CommandPaletteOutcome::Execute(id))
        } else {
            Ok(outcome)
        }
    }
    pub fn accessibility_tree(&self) -> sol_ui::AccessibilityNode {
        self.tree.accessibility_tree()
    }
    /// Return the dedicated accessibility projection for the transient palette.
    #[must_use]
    pub fn command_palette_accessibility_tree(&self) -> AccessibilityNode {
        self.palette.accessibility_tree()
    }
    fn apply(&mut self, change: SettingsChange) -> Result<(), SettingsError> {
        self.snapshot = self.api.apply(change)?;
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    use sol_settingsd::{FileSettingsStore, SettingsDaemon};
    let path = std::env::var("SOL_SETTINGS_PATH").unwrap_or_else(|_| "./sol-settings.conf".into());
    let mut settings = SettingsApp::new(SettingsDaemon::new(FileSettingsStore::new(path))?)?;
    settings.app.start()?;
    println!(
        "SOL Settings — {} (revision {})",
        settings.page().title(),
        settings.snapshot().revision
    );
    println!(
        "Commands: {:?}",
        SettingsApp::<SettingsDaemon<FileSettingsStore>>::commands("")
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sol_design::accessibility::{Contrast, MotionPreference, Theme};
    use sol_settingsd::{MemorySettingsStore, SettingsDaemon};
    fn app() -> SettingsApp<SettingsDaemon<MemorySettingsStore>> {
        SettingsApp::new(SettingsDaemon::new(MemorySettingsStore::new()).unwrap()).unwrap()
    }
    #[test]
    fn appearance_commands_round_trip_and_resolve_accessibility_tokens() {
        let mut app = app();
        app.execute("appearance.dark").unwrap();
        app.execute("appearance.high_contrast").unwrap();
        app.execute("appearance.reduced_motion").unwrap();
        app.execute("appearance.text_large").unwrap();
        let mode = app.token_mode();
        assert_eq!(mode.theme, Theme::Dark);
        assert_eq!(mode.contrast, Contrast::High);
        assert_eq!(mode.motion, MotionPreference::Reduced);
        assert_eq!(mode.text_scale, UiTextScale::Large);
        assert_eq!(app.snapshot().revision, 4);
    }
    #[test]
    fn sound_and_unavailable_pages_are_truthful() {
        let mut app = app();
        app.set_volume(73).unwrap();
        app.execute("sound.mute").unwrap();
        app.execute("page.displays").unwrap();
        assert_eq!(app.snapshot().audio.output_volume.percent(), 73);
        assert!(app.snapshot().audio.output_muted);
        assert!(app.unavailable_message().unwrap().contains("unavailable"));
    }
    #[test]
    fn palette_and_keyboard_accessibility_are_deterministic() {
        let mut app = app();
        assert_eq!(
            SettingsApp::<SettingsDaemon<MemorySettingsStore>>::commands("contrast").len(),
            1
        );
        assert!(matches!(
            app.handle_key(Key::Tab),
            KeyboardOutcome::FocusMoved(_)
        ));
        assert!(app.accessibility_tree().children[0].state.focused);
    }
    #[test]
    fn shared_palette_executes_settings_commands_and_projects_empty_state() {
        let mut app = app();
        assert!(matches!(
            app.handle_command_palette_key(Key::CommandPalette).unwrap(),
            CommandPaletteOutcome::Opened
        ));
        app.handle_command_palette_key(Key::Tab).unwrap();
        assert_eq!(
            app.handle_command_palette_key(Key::Enter).unwrap(),
            CommandPaletteOutcome::Execute("appearance.dark")
        );
        assert_eq!(app.token_mode().theme, Theme::Dark);
        app.handle_command_palette_key(Key::ShiftTab).unwrap();
        app.handle_command_palette_key(Key::Character('z')).unwrap();
        assert_eq!(
            app.command_palette_accessibility_tree().children[1].label,
            "No matching commands"
        );
        assert_eq!(
            app.handle_command_palette_key(Key::Escape).unwrap(),
            CommandPaletteOutcome::Closed
        );
    }
}
