//! A small, complete SolKit application workflow.
//!
//! This crate intentionally imports only SolKit public crates. The default
//! execution path is deterministic and headless; the `native` feature opens
//! the same semantic view through `sol-ui` without naming a renderer backend.

use sol_animation::AnimationEffect;
use sol_app::{App, AppId, AppWindow, Command, CommandContext, CommandRegistry, CommandResult};
use sol_design::{
    accessibility::{TextScale, TokenMode},
    color::Color,
    spacing::Spacing,
};
use sol_graphics::{GraphicsContext, Surface};
use sol_ui::{
    Button, ButtonController, HStack, InteractionTree, Key, KeyboardOutcome, Renderer,
    SemanticControl, Tab, TabBar, TextField, Toolbar, ToolbarItem, VStack,
};

/// The example's stable application ID.
pub const APP_ID: &str = "org.sol.examples.showcase";

/// A deterministic result proving the complete SolKit flow ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShowcaseReport {
    /// The command result emitted through `sol-app`.
    pub command_data: String,
    /// The tab selected by keyboard navigation.
    pub selected_tab: String,
    /// The search text retained after keyboard editing.
    pub search_value: String,
    /// The activated button's semantic identifier.
    pub activated_control: String,
    /// The token-only visual contract used by the primary component.
    pub visual_contract: String,
    /// The reduced-motion duration resolved by `sol-design`.
    pub animation_duration_ms: u32,
}

/// A complete app composed only from SolKit contracts.
pub struct ShowcaseApp {
    app: App,
    commands: CommandRegistry,
    token_mode: TokenMode,
    primary_button: ButtonController,
    interactions: InteractionTree,
    #[allow(dead_code)]
    toolbar: Toolbar,
    #[allow(dead_code)]
    tabs: TabBar,
    #[allow(dead_code)]
    layout: VStack,
    #[allow(dead_code)]
    inline_actions: HStack,
    #[allow(dead_code)]
    search: TextField,
}

impl ShowcaseApp {
    /// Build the complete semantic application without constructing a backend.
    pub fn new() -> Result<Self, String> {
        let id = AppId::parse(APP_ID).map_err(|error| error.to_string())?;
        let mut app = App::new(id);
        app.add_window(AppWindow::new("SolKit Showcase"));

        let open = Button::new().with_label("Open");
        let search = TextField::new().with_placeholder("Search settings");
        let general = Tab::new("General").select();
        let advanced = Tab::new("Advanced");

        let toolbar = Toolbar::new()
            .item(ToolbarItem::Button(Button::new().with_label("Open")))
            .item(ToolbarItem::Separator)
            .item(ToolbarItem::Label("Showcase"));
        let tabs = TabBar::new().tab(general).tab(advanced);
        let inline_actions = HStack::new()
            .spacing(Spacing::Sm)
            .item(sol_ui::StackItem::Button(Button::new().with_label("Open")))
            .item(sol_ui::StackItem::Spacer);
        let layout = VStack::new()
            .spacing(Spacing::Lg)
            .item(sol_ui::StackItem::Button(Button::new().with_label("Open")));

        let mut interactions = InteractionTree::new("showcase", "SolKit Showcase");
        interactions.push(SemanticControl::button("open", &open));
        interactions.push(SemanticControl::text_field("search", &search));
        interactions.push(SemanticControl::tab(
            "general",
            &Tab::new("General").select(),
        ));
        interactions.push(SemanticControl::tab("advanced", &Tab::new("Advanced")));

        let mut commands = CommandRegistry::new();
        commands.register(OpenCommand);

        Ok(Self {
            app,
            commands,
            token_mode: TokenMode::dark()
                .high_contrast()
                .reduced_motion()
                .with_text_scale(TextScale::Large),
            primary_button: ButtonController::new(open),
            interactions,
            toolbar,
            tabs,
            layout,
            inline_actions,
            search,
        })
    }

    /// Execute one deterministic user workflow through SolKit only.
    pub fn run_headless(&mut self) -> Result<ShowcaseReport, String> {
        self.app.start().map_err(|error| error.to_string())?;
        let command = self
            .commands
            .execute("file.open", CommandContext::default());
        let command_data = command
            .data
            .ok_or_else(|| "open command returned no data".to_owned())?;

        self.expect_outcome(Key::Tab, "focus open")?;
        let activated_control = match self.interactions.handle_key(Key::Enter) {
            KeyboardOutcome::Activated(id) => id.as_str().to_owned(),
            outcome => return Err(format!("expected button activation, got {outcome:?}")),
        };
        self.expect_outcome(Key::Tab, "focus search")?;
        self.expect_outcome(Key::Character('S'), "edit search")?;
        self.expect_outcome(Key::Character('O'), "edit search")?;
        self.expect_outcome(Key::Tab, "focus general tab")?;
        let selected_tab = match self.interactions.handle_key(Key::ArrowRight) {
            KeyboardOutcome::SelectionChanged(id) => id.as_str().to_owned(),
            outcome => return Err(format!("expected tab selection, got {outcome:?}")),
        };

        self.primary_button.take_over_with_progress(0.5);
        let animation_duration_ms = self
            .animation()
            .spec()
            .duration_ms
            .min(self.primary_button.motion_spec(self.token_mode).duration_ms);

        let tree = self.interactions.accessibility_tree();
        let search_value = tree.children[1].value.clone().unwrap_or_default();

        let mut graphics = GraphicsContext::new(Surface::default());
        graphics.prepare();
        graphics.clear(Color::Surface);
        graphics.present();

        Ok(ShowcaseReport {
            command_data,
            selected_tab,
            search_value,
            activated_control,
            visual_contract: Button::new().visual_tokens().snapshot(),
            animation_duration_ms,
        })
    }

    /// Render the primary semantic component through any SolUI renderer.
    pub fn render_with(&self, renderer: &mut impl Renderer) {
        renderer.render_button(&self.primary_button.frame_for(self.token_mode));
    }

    /// Return the semantic tree for a future accessibility bridge.
    pub fn accessibility_tree(&self) -> sol_ui::AccessibilityNode {
        self.interactions.accessibility_tree()
    }

    fn animation(&self) -> AnimationEffect {
        AnimationEffect::panel()
    }

    fn expect_outcome(&mut self, key: Key, action: &str) -> Result<(), String> {
        match self.interactions.handle_key(key) {
            KeyboardOutcome::FocusMoved(_) | KeyboardOutcome::TextChanged(_) => Ok(()),
            outcome => Err(format!("expected {action}, got {outcome:?}")),
        }
    }
}

struct OpenCommand;

impl Command for OpenCommand {
    fn id(&self) -> &'static str {
        "file.open"
    }

    fn title(&self) -> &'static str {
        "Open"
    }

    fn execute(&self, _ctx: CommandContext) -> CommandResult {
        CommandResult::success(Some("opened through SolKit".to_owned()))
    }
}

/// Run the complete deterministic workflow used by the CLI and tests.
pub fn run_headless_showcase() -> Result<ShowcaseReport, String> {
    ShowcaseApp::new()?.run_headless()
}

/// Run the semantic view through SolUI's optional native adapter.
#[cfg(feature = "native")]
pub fn run_native_showcase() -> Result<(), String> {
    let mut showcase = ShowcaseApp::new()?;
    let _ = showcase.run_headless()?;
    let mut renderer = sol_ui::NativeRenderer::new()?;
    showcase.render_with(&mut renderer);
    renderer.run()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_workflow_uses_commands_animation_and_keyboard_navigation() {
        let report = run_headless_showcase().expect("showcase workflow should run");

        assert_eq!(report.command_data, "opened through SolKit");
        assert_eq!(report.activated_control, "open");
        assert_eq!(report.search_value, "SO");
        assert_eq!(report.selected_tab, "advanced");
        assert_eq!(report.animation_duration_ms, 0);
        assert_eq!(
            report.visual_contract,
            "background=Elevated;foreground=TextPrimary;padding=Md;radius=Sm;metric=Button;motion=Fast;typography=Label"
        );
    }

    #[test]
    fn accessibility_tree_is_available_without_a_renderer() {
        let showcase = ShowcaseApp::new().expect("showcase should construct");
        let tree = showcase.accessibility_tree();

        assert_eq!(tree.label, "SolKit Showcase");
        assert_eq!(tree.children.len(), 4);
    }

    #[test]
    fn manifest_never_names_a_concrete_renderer_or_backend() {
        let manifest = include_str!("../Cargo.toml");
        for forbidden in ["slint", "winit", "wayland", "smithay", "wgpu", "vulkan"] {
            assert!(!manifest.contains(forbidden), "manifest leaked {forbidden}");
        }
    }
}
