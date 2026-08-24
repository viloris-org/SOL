//! Live-session installation welcome page.
//!
//! This crate owns only the user-facing entry into installation. It does not
//! write disks or claim that the Phase 7 installation transaction exists.

use sol_app::{App, AppId, AppWindow};
use sol_design::accessibility::TokenMode;
use sol_ui::{
    AccessibilityNode, Button, GuidedPage, GuidedPageFrame, GuidedPageStep, InteractionTree,
    SemanticControl,
};

/// Stable application identity used by the live image.
pub const APP_ID: &str = "org.sol.installer";

/// Trusted context supplied by the live-session launcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveSessionInfo {
    /// Human-readable SOL release name.
    pub release: String,
    /// Whether the live image can continue without network access.
    pub offline_ready: bool,
}

impl LiveSessionInfo {
    /// Construct context for one verified live image.
    pub fn new(release: impl Into<String>, offline_ready: bool) -> Self {
        Self {
            release: release.into(),
            offline_ready,
        }
    }
}

/// The two intentionally reversible exits from the welcome page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallGuideOutcome {
    /// Hand off to the disk and security decision flow.
    BeginInstallation,
    /// Close the guide without changing the machine.
    KeepExploring,
}

/// Renderer-neutral state for the live installation entry page.
pub struct InstallerWelcome {
    /// Standard SOL application lifecycle owner.
    pub app: App,
    info: LiveSessionInfo,
    page: GuidedPage,
    interactions: InteractionTree,
}

impl InstallerWelcome {
    /// Build the page from authenticated live-image metadata.
    pub fn new(info: LiveSessionInfo) -> Result<Self, String> {
        let id = AppId::parse(APP_ID).map_err(|error| error.to_string())?;
        let mut app = App::new(id);
        app.add_window(AppWindow::new("Install SOL"));

        let offline_highlight = if info.offline_ready {
            "The included release can be installed offline"
        } else {
            "Connect to a network before installation"
        };
        let page = GuidedPage::new(
            "LIVE SESSION",
            "Ready to make SOL yours?",
            format!(
                "You are exploring {} from a temporary live environment. Take your time: installation starts only after you choose a disk and approve the final plan.",
                info.release
            ),
            "Install SOL",
            "Keep exploring",
        )
        .highlight("Your disks have not been changed")
        .highlight(offline_highlight)
        .highlight("You can review every choice before installation")
        .step(
            GuidedPageStep::new(
                "Choose where SOL lives",
                "Select a destination and see the exact disk layout.",
            )
            .current(),
        )
        .step(GuidedPageStep::new(
            "Protect your data",
            "Configure encryption, recovery access, and Secure Boot.",
        ))
        .step(GuidedPageStep::new(
            "Review and install",
            "Confirm the plan before any change is committed.",
        ));

        let mut interactions = InteractionTree::new("installer-welcome", "Install SOL");
        interactions.push(SemanticControl::button(
            "installer.begin",
            &Button::new().with_label("Install SOL").primary(),
        ));
        interactions.push(SemanticControl::button(
            "installer.explore",
            &Button::new().with_label("Keep exploring"),
        ));

        Ok(Self {
            app,
            info,
            page,
            interactions,
        })
    }

    /// Return the release metadata represented by this page.
    pub const fn live_session(&self) -> &LiveSessionInfo {
        &self.info
    }

    /// Resolve the complete visual projection using the active SOL tokens.
    pub fn frame_for(&self, mode: TokenMode) -> GuidedPageFrame {
        self.page.frame_for(mode)
    }

    /// Expose both exits to keyboard and assistive-technology integrations.
    pub fn accessibility_tree(&self) -> AccessibilityNode {
        self.interactions.accessibility_tree()
    }

    /// Translate one page exit into an application-level outcome.
    pub const fn choose(primary: bool) -> InstallGuideOutcome {
        if primary {
            InstallGuideOutcome::BeginInstallation
        } else {
            InstallGuideOutcome::KeepExploring
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sol_design::{accessibility::Theme, color::Color};
    use sol_ui::{GuidedStepState, SemanticRole};

    fn welcome() -> InstallerWelcome {
        InstallerWelcome::new(LiveSessionInfo::new("SOL Preview 0.1", true))
            .expect("valid installer welcome")
    }

    #[test]
    fn live_page_is_truthful_and_has_two_explicit_exits() {
        let welcome = welcome();
        let frame = welcome.frame_for(TokenMode::light());
        let tree = welcome.accessibility_tree();

        assert!(frame.description.contains("temporary live environment"));
        assert!(frame.highlights[0].contains("not been changed"));
        assert_eq!(frame.primary.label, "Install SOL");
        assert_eq!(frame.secondary.label, "Keep exploring");
        assert_eq!(tree.children.len(), 2);
        assert!(
            tree.children
                .iter()
                .all(|node| node.role == SemanticRole::Button)
        );
    }

    #[test]
    fn overview_matches_the_required_install_decisions() {
        let frame = welcome().frame_for(TokenMode::light());

        assert_eq!(frame.steps.len(), 3);
        assert_eq!(frame.steps[0].state, GuidedStepState::Current);
        assert!(frame.steps[0].description.contains("disk layout"));
        assert!(frame.steps[1].description.contains("Secure Boot"));
        assert!(frame.steps[2].description.contains("Confirm"));
    }

    #[test]
    fn page_follows_the_selected_token_mode() {
        let mode = TokenMode::dark();
        let frame = welcome().frame_for(mode);

        assert_eq!(mode.theme, Theme::Dark);
        assert_eq!(frame.page_background, mode.color(Color::Surface));
        assert_eq!(frame.primary.background, mode.color(Color::Accent));
    }

    #[test]
    fn application_manifest_keeps_the_native_backend_private() {
        let manifest = include_str!("../Cargo.toml");
        for forbidden in ["slint", "winit", "wayland", "smithay"] {
            assert!(!manifest.contains(forbidden), "manifest leaked {forbidden}");
        }
    }
}
