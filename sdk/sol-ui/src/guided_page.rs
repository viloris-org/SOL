//! A reusable, renderer-neutral welcome page for guided system flows.

use sol_design::{
    accessibility::TokenMode,
    color::{Color, Rgba},
    radius::Radius,
    spacing::Spacing,
    typography::FontStyle,
};

use crate::{Button, ButtonController};

/// Progress state shown beside one overview step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuidedStepState {
    /// The step is the next part of the flow.
    Current,
    /// The step follows a preceding step.
    Upcoming,
    /// The step has already been completed.
    Complete,
}

/// One concise step in the page's process overview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuidedPageStep {
    /// Short step title.
    pub title: String,
    /// One-line explanation of the decision or result.
    pub description: String,
    /// Current progress state.
    pub state: GuidedStepState,
}

impl GuidedPageStep {
    /// Create an upcoming step.
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
            state: GuidedStepState::Upcoming,
        }
    }

    /// Mark this as the current step.
    pub const fn current(mut self) -> Self {
        self.state = GuidedStepState::Current;
        self
    }

    /// Mark this step as already completed.
    pub const fn complete(mut self) -> Self {
        self.state = GuidedStepState::Complete;
        self
    }
}

/// Semantic source of truth for a full-window guided welcome page.
pub struct GuidedPage {
    /// Small context label above the title.
    pub eyebrow: String,
    /// Primary page heading.
    pub title: String,
    /// Supporting explanation.
    pub description: String,
    /// Reassuring facts shown beside the primary choice.
    pub highlights: Vec<String>,
    /// Process overview shown in the side card.
    pub steps: Vec<GuidedPageStep>,
    /// Preferred action.
    pub primary_action: ButtonController,
    /// Safe exit or deferral action.
    pub secondary_action: ButtonController,
}

impl GuidedPage {
    /// Create a page with its two explicit exits.
    pub fn new(
        eyebrow: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
        primary_label: &'static str,
        secondary_label: &'static str,
    ) -> Self {
        Self {
            eyebrow: eyebrow.into(),
            title: title.into(),
            description: description.into(),
            highlights: Vec::new(),
            steps: Vec::new(),
            primary_action: ButtonController::new(
                Button::new().with_label(primary_label).primary(),
            ),
            secondary_action: ButtonController::new(Button::new().with_label(secondary_label)),
        }
    }

    /// Add one reassuring fact.
    pub fn highlight(mut self, highlight: impl Into<String>) -> Self {
        self.highlights.push(highlight.into());
        self
    }

    /// Add one process overview step.
    pub fn step(mut self, step: GuidedPageStep) -> Self {
        self.steps.push(step);
        self
    }

    /// Resolve the semantic page into a backend-neutral native frame.
    pub fn frame_for(&self, mode: TokenMode) -> GuidedPageFrame {
        GuidedPageFrame {
            eyebrow: self.eyebrow.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
            highlights: self.highlights.clone(),
            steps: self.steps.clone(),
            primary: self.primary_action.frame_for(mode),
            secondary: self.secondary_action.frame_for(mode),
            page_background: mode.color(Color::Surface),
            panel_background: mode.color(Color::Elevated),
            text_primary: mode.color(Color::TextPrimary),
            text_secondary: mode.color(Color::TextSecondary),
            accent: mode.color(Color::Accent),
            border: mode.color(Color::Border),
            display_size: mode.typography(FontStyle::Display).pixels,
            title_size: mode.typography(FontStyle::Title).pixels,
            body_size: mode.typography(FontStyle::Body).pixels,
            label_size: mode.typography(FontStyle::Label).pixels,
            control_radius: Radius::Sm.px(),
            panel_radius: Radius::Md.px(),
            spacing_small: Spacing::Sm.px(),
            spacing_medium: Spacing::Md.px(),
            spacing_large: Spacing::Lg.px(),
            spacing_xlarge: Spacing::Xl.px(),
        }
    }
}

/// Fully resolved guided-page frame consumed by a rendering adapter.
#[derive(Debug, Clone, PartialEq)]
pub struct GuidedPageFrame {
    pub eyebrow: String,
    pub title: String,
    pub description: String,
    pub highlights: Vec<String>,
    pub steps: Vec<GuidedPageStep>,
    pub primary: crate::ButtonFrame,
    pub secondary: crate::ButtonFrame,
    pub page_background: Rgba,
    pub panel_background: Rgba,
    pub text_primary: Rgba,
    pub text_secondary: Rgba,
    pub accent: Rgba,
    pub border: Rgba,
    pub display_size: f32,
    pub title_size: f32,
    pub body_size: f32,
    pub label_size: f32,
    pub control_radius: f32,
    pub panel_radius: f32,
    pub spacing_small: f32,
    pub spacing_medium: f32,
    pub spacing_large: f32,
    pub spacing_xlarge: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_resolves_copy_steps_and_tokens_in_one_frame() {
        let page = GuidedPage::new(
            "LIVE",
            "Welcome",
            "No disk changes yet.",
            "Continue",
            "Later",
        )
        .highlight("Safe to explore")
        .step(GuidedPageStep::new("Choose", "Select a disk").current());
        let mode = TokenMode::dark();
        let frame = page.frame_for(mode);

        assert_eq!(frame.highlights, ["Safe to explore"]);
        assert_eq!(frame.steps.len(), 1);
        assert_eq!(frame.primary.background, mode.color(Color::Accent));
        assert_eq!(frame.page_background, mode.color(Color::Surface));
    }
}
