//! A minimal, renderer-independent SolKit application.

use sol_app::{App, AppId, AppWindow, Command, CommandContext, CommandRegistry, CommandResult};
use sol_design::accessibility::TokenMode;
use sol_ui::{Button, InteractionTree, Key, KeyboardOutcome, SemanticControl};

/// Replace this identifier with one owned by the application publisher.
pub const APP_ID: &str = "org.example.starter";

/// The stable result produced by the starter workflow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StarterReport {
    pub command_result: String,
    pub activated_control: String,
    pub token_contract: String,
    pub reduced_motion_duration_ms: u32,
}

/// Construct and run one renderer-independent application workflow.
pub fn run() -> Result<StarterReport, String> {
    let id = AppId::parse(APP_ID).map_err(|error| error.to_string())?;
    let mut app = App::new(id);
    app.add_window(AppWindow::new("SolKit Starter"));
    app.start().map_err(|error| error.to_string())?;

    let open = Button::new().with_label("Open");
    let mut commands = CommandRegistry::new();
    commands.register(OpenCommand);
    let command_result = commands
        .execute("app.open", CommandContext::default())
        .data
        .ok_or_else(|| "open command returned no data".to_owned())?;

    let mut interactions = InteractionTree::new("starter", "SolKit Starter");
    interactions.push(SemanticControl::button("open", &open));
    let activated_control = match interactions.handle_key(Key::Tab) {
        KeyboardOutcome::FocusMoved(_) => match interactions.handle_key(Key::Enter) {
            KeyboardOutcome::Activated(id) => id.as_str().to_owned(),
            outcome => return Err(format!("expected activation, got {outcome:?}")),
        },
        outcome => return Err(format!("expected focus, got {outcome:?}")),
    };

    Ok(StarterReport {
        command_result,
        activated_control,
        token_contract: open.visual_tokens().snapshot(),
        reduced_motion_duration_ms: TokenMode::dark()
            .reduced_motion()
            .motion_spec(open.motion())
            .duration_ms,
    })
}

struct OpenCommand;

impl Command for OpenCommand {
    fn id(&self) -> &'static str {
        "app.open"
    }

    fn title(&self) -> &'static str {
        "Open"
    }

    fn execute(&self, _context: CommandContext) -> CommandResult {
        CommandResult::success(Some("opened".to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_workflow_is_deterministic_and_token_only() {
        let report = run().expect("starter workflow should run");

        assert_eq!(report.command_result, "opened");
        assert_eq!(report.activated_control, "open");
        assert_eq!(report.reduced_motion_duration_ms, 0);
        assert_eq!(
            report.token_contract,
            "background=Elevated;foreground=TextPrimary;padding=Md;radius=Sm;metric=Button;motion=Fast;typography=Label"
        );
    }

    #[test]
    fn manifest_uses_only_public_solkit_crates() {
        let manifest = include_str!("../Cargo.toml");
        for forbidden in ["slint", "winit", "wayland", "smithay", "wgpu", "vulkan"] {
            assert!(!manifest.contains(forbidden), "manifest leaked {forbidden}");
        }
    }
}
