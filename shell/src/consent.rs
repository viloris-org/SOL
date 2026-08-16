//! Renderer-neutral trusted consent prompt.
//!
//! The prompt renders an attributed [`sol_system::UserConsentRequest`] and
//! returns only typed decisions to [`sol_system::SystemActionApi`]. It never
//! performs the requested system action itself.

use sol_system::{
    ActionAuthorization, ActionDenial, ActionError, ActionSource, ConsentDecision, SystemAction,
    SystemActionApi, SystemActionResult, UserConsentRequest,
};
use sol_ui::{AccessibilityNode, Button, InteractionTree, Key, KeyboardOutcome, SemanticControl};
use std::error::Error;
use std::fmt;

const ALLOW_ONCE_ID: &str = "consent.allow-once";
const ALLOW_ALWAYS_ID: &str = "consent.allow-always";
const DENY_ID: &str = "consent.deny";

/// A decision exposed by the trusted consent prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentChoice {
    /// Authorize only the displayed request.
    AllowOnce,
    /// Authorize this caller/capability pair until the grant is revoked.
    AllowAlways,
    /// Deny this caller/capability pair until the grant is revoked.
    Deny,
}

impl ConsentChoice {
    const fn semantic_id(self) -> &'static str {
        match self {
            Self::AllowOnce => ALLOW_ONCE_ID,
            Self::AllowAlways => ALLOW_ALWAYS_ID,
            Self::Deny => DENY_ID,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::AllowOnce => "Allow once",
            Self::AllowAlways => "Always allow",
            Self::Deny => "Deny and remember",
        }
    }

    const fn decision(self) -> ConsentDecision {
        match self {
            Self::AllowOnce => ConsentDecision::AllowOnce,
            Self::AllowAlways => ConsentDecision::AllowAlways,
            Self::Deny => ConsentDecision::Deny,
        }
    }

    fn from_semantic_id(id: &str) -> Option<Self> {
        match id {
            ALLOW_ONCE_ID => Some(Self::AllowOnce),
            ALLOW_ALWAYS_ID => Some(Self::AllowAlways),
            DENY_ID => Some(Self::Deny),
            _ => None,
        }
    }
}

/// Stable content projected by a native trusted consent surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsentPromptView {
    /// Authorization-service request identifier.
    pub request_id: u64,
    /// Opaque consent identifier returned unchanged when resolving the prompt.
    pub consent_id: u64,
    /// Validated application identity requesting access.
    pub caller: String,
    /// Shell or portal surface that originated the request.
    pub source: &'static str,
    /// Least-privilege capability being requested.
    pub capability: &'static str,
    /// Exact typed action that would become authorized.
    pub action: String,
    /// Policy-owned explanation shown to the user.
    pub rationale: String,
    /// Keyboard and assistive-technology projection for the choices.
    pub accessibility: AccessibilityNode,
}

/// Completed prompt result. Authorization still must be consumed by a
/// concrete system adapter; this model does not execute it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsentPromptOutcome {
    /// Keyboard focus moved to a prompt choice.
    FocusMoved(ConsentChoice),
    /// The displayed request was authorized.
    Authorized(ActionAuthorization),
    /// The displayed request was denied.
    Denied(ActionDenial),
    /// The key did not apply to this prompt.
    Ignored,
}

/// Failure returned by the trusted consent prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsentPromptError {
    /// The authorization service rejected or could not persist the decision.
    Authorization(ActionError),
    /// The prompt has already consumed its one displayed consent request.
    AlreadyResolved,
    /// The authorization implementation returned another pending prompt while
    /// resolving the displayed consent ID.
    StillAwaitingConsent,
}

impl fmt::Display for ConsentPromptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Authorization(error) => write!(formatter, "consent resolution failed: {error}"),
            Self::AlreadyResolved => formatter.write_str("consent prompt is already resolved"),
            Self::StillAwaitingConsent => {
                formatter.write_str("consent resolution returned another pending prompt")
            }
        }
    }
}

impl Error for ConsentPromptError {}

impl From<ActionError> for ConsentPromptError {
    fn from(error: ActionError) -> Self {
        Self::Authorization(error)
    }
}

/// Renderer-neutral prompt bound to one exact authorization-service request.
pub struct ConsentPrompt<A: SystemActionApi> {
    actions: A,
    request: UserConsentRequest,
    interactions: InteractionTree,
    resolved: bool,
}

impl<A: SystemActionApi> ConsentPrompt<A> {
    /// Create a trusted prompt for an authorization-service consent request.
    #[must_use]
    pub fn new(actions: A, request: UserConsentRequest) -> Self {
        let mut interactions = InteractionTree::new("consent", "System permission request");
        for choice in [
            ConsentChoice::AllowOnce,
            ConsentChoice::AllowAlways,
            ConsentChoice::Deny,
        ] {
            let button = Button::new().with_label(choice.label());
            interactions.push(SemanticControl::button(choice.semantic_id(), &button));
        }
        Self {
            actions,
            request,
            interactions,
            resolved: false,
        }
    }

    /// Return the exact attributed request and semantic choice tree.
    #[must_use]
    pub fn view(&self) -> ConsentPromptView {
        let caller = self.request.caller.to_string();
        let action = action_summary(&self.request.action);
        let mut accessibility = self.interactions.accessibility_tree();
        accessibility.label = format!(
            "System permission request from {caller}. {action}. {}",
            self.request.rationale
        );
        ConsentPromptView {
            request_id: self.request.request_id.get(),
            consent_id: self.request.consent_id.get(),
            caller,
            source: action_source_name(self.request.source),
            capability: self.request.capability.as_str(),
            action,
            rationale: self.request.rationale.clone(),
            accessibility,
        }
    }

    /// Resolve a direct pointer/touch choice through the authorization API.
    pub fn choose(
        &mut self,
        choice: ConsentChoice,
    ) -> Result<ConsentPromptOutcome, ConsentPromptError> {
        if self.resolved {
            return Err(ConsentPromptError::AlreadyResolved);
        }
        let result = self
            .actions
            .resolve_user_consent(self.request.consent_id, choice.decision())?;
        self.resolved = true;
        match result {
            SystemActionResult::Authorized(authorization) => {
                Ok(ConsentPromptOutcome::Authorized(authorization))
            }
            SystemActionResult::Denied { reason, .. } => Ok(ConsentPromptOutcome::Denied(reason)),
            SystemActionResult::AwaitingUserConsent(_) => {
                Err(ConsentPromptError::StillAwaitingConsent)
            }
        }
    }

    /// Handle normalized keyboard input using SolUI focus and activation rules.
    pub fn handle_key(&mut self, key: Key) -> Result<ConsentPromptOutcome, ConsentPromptError> {
        match self.interactions.handle_key(key) {
            KeyboardOutcome::FocusMoved(id) => Ok(ConsentChoice::from_semantic_id(id.as_str())
                .map_or(
                    ConsentPromptOutcome::Ignored,
                    ConsentPromptOutcome::FocusMoved,
                )),
            KeyboardOutcome::Activated(id) => ConsentChoice::from_semantic_id(id.as_str())
                .map_or(Ok(ConsentPromptOutcome::Ignored), |choice| {
                    self.choose(choice)
                }),
            _ => Ok(ConsentPromptOutcome::Ignored),
        }
    }
}

const fn action_source_name(source: ActionSource) -> &'static str {
    match source {
        ActionSource::ShellLauncher => "shell-launcher",
        ActionSource::Search => "search",
        ActionSource::QuickSettings => "quick-settings",
        ActionSource::Notifications => "notifications",
        ActionSource::Portal => "portal",
        ActionSource::Accessibility => "accessibility",
        ActionSource::Automation => "automation",
        ActionSource::AiOrVoice => "ai-or-voice",
    }
}

fn action_summary(action: &SystemAction) -> String {
    match action {
        SystemAction::LaunchApplication { app_id } => format!("launch application {app_id}"),
        SystemAction::Search { query } => format!("search for {query}"),
        SystemAction::SetOutputVolume { volume } => {
            format!("set output volume to {}%", volume.percent())
        }
        SystemAction::SetOutputMuted { muted } => format!("set output muted to {muted}"),
        SystemAction::InvokeNotificationAction {
            notification_id,
            action_id,
        } => format!(
            "invoke notification {} action {}",
            notification_id,
            action_id.as_str()
        ),
        SystemAction::RequestScreenCapture => "request screen capture".to_owned(),
        SystemAction::OpenDocument { uri } => format!("open document {uri}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sol_system::{
        ActionSource, AppId, MemoryActionAuditStore, MemoryPermissionStore, PermissionDecision,
        PermissionPolicy, PolicyDecision, SystemActionRequest, SystemActionService,
        SystemCapability,
    };

    #[derive(Debug, Clone, Copy)]
    struct PromptPolicy;

    impl PermissionPolicy for PromptPolicy {
        fn decide(
            &self,
            _request: &SystemActionRequest,
            _capability: SystemCapability,
        ) -> PolicyDecision {
            PolicyDecision::RequireUserConsent {
                rationale: "This application wants to change output audio.".to_owned(),
            }
        }
    }

    type Actions = SystemActionService<PromptPolicy, MemoryPermissionStore, MemoryActionAuditStore>;

    fn prompt() -> ConsentPrompt<Actions> {
        let actions = SystemActionService::new(
            PromptPolicy,
            MemoryPermissionStore::default(),
            MemoryActionAuditStore::default(),
        );
        let request = SystemActionRequest {
            caller: AppId::parse("com.example.mixer").expect("valid fixture app ID"),
            source: ActionSource::QuickSettings,
            action: SystemAction::SetOutputMuted { muted: true },
        };
        let SystemActionResult::AwaitingUserConsent(consent) = actions
            .request(request)
            .expect("request should reach consent")
        else {
            panic!("fixture policy must require consent");
        };
        ConsentPrompt::new(actions, consent)
    }

    #[test]
    fn view_preserves_attribution_action_and_accessible_choices() {
        let prompt = prompt();
        let view = prompt.view();

        assert_eq!(view.request_id, 1);
        assert_eq!(view.consent_id, 1);
        assert_eq!(view.caller, "com.example.mixer");
        assert_eq!(view.source, "quick-settings");
        assert_eq!(view.capability, "change-quick-settings");
        assert_eq!(view.action, "set output muted to true");
        assert_eq!(
            view.rationale,
            "This application wants to change output audio."
        );
        assert_eq!(view.accessibility.children.len(), 3);
        assert_eq!(view.accessibility.children[0].label, "Allow once");
        assert_eq!(view.accessibility.children[1].label, "Always allow");
        assert_eq!(view.accessibility.children[2].label, "Deny and remember");
        assert!(view.accessibility.label.contains("com.example.mixer"));
        assert!(
            view.accessibility
                .label
                .contains("set output muted to true")
        );
    }

    #[test]
    fn keyboard_allow_once_resolves_exact_request_and_audits_it() {
        let mut prompt = prompt();

        assert_eq!(
            prompt.handle_key(Key::Tab).expect("focus should move"),
            ConsentPromptOutcome::FocusMoved(ConsentChoice::AllowOnce)
        );
        let ConsentPromptOutcome::Authorized(authorization) = prompt
            .handle_key(Key::Enter)
            .expect("allow once should resolve")
        else {
            panic!("allow once must authorize the displayed request");
        };
        assert_eq!(authorization.request_id.get(), 1);
        assert_eq!(
            authorization.request.caller.to_string(),
            "com.example.mixer"
        );
        assert_eq!(
            authorization.request.action,
            SystemAction::SetOutputMuted { muted: true }
        );
        assert_eq!(
            prompt.actions.audit_records().expect("audit should load")[1].decision,
            PermissionDecision::Authorized
        );
        assert_eq!(
            prompt.choose(ConsentChoice::Deny),
            Err(ConsentPromptError::AlreadyResolved)
        );
    }

    #[test]
    fn allow_always_and_deny_persist_caller_scoped_grants() {
        let mut allowed = prompt();
        assert!(matches!(
            allowed
                .choose(ConsentChoice::AllowAlways)
                .expect("allow always should resolve"),
            ConsentPromptOutcome::Authorized(_)
        ));
        assert!(matches!(
            allowed
                .actions
                .request(SystemActionRequest {
                    caller: AppId::parse("com.example.mixer").expect("valid fixture app ID"),
                    source: ActionSource::QuickSettings,
                    action: SystemAction::SetOutputMuted { muted: false },
                })
                .expect("stored allow should evaluate"),
            SystemActionResult::Authorized(_)
        ));

        let mut denied = prompt();
        assert_eq!(
            denied
                .choose(ConsentChoice::Deny)
                .expect("deny should resolve"),
            ConsentPromptOutcome::Denied(ActionDenial::UserDenied)
        );
        assert!(matches!(
            denied
                .actions
                .request(SystemActionRequest {
                    caller: AppId::parse("com.example.mixer").expect("valid fixture app ID"),
                    source: ActionSource::QuickSettings,
                    action: SystemAction::SetOutputMuted { muted: false },
                })
                .expect("stored deny should evaluate"),
            SystemActionResult::Denied {
                reason: ActionDenial::StoredDeny,
                ..
            }
        ));
    }
}
