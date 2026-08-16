//! Renderer-neutral Notification Center model.
//!
//! This module owns grouping, focus, keyboard, and accessibility behavior. It
//! receives all notification state and lifecycle mutations through the stable
//! [`sol_system::NotificationApi`] service boundary; a future D-Bus proxy is
//! an adapter, not a replacement for this model.

use sol_design::{
    color::Color, metrics::ControlMetric, motion::Motion, radius::Radius, spacing::Spacing,
    typography::FontStyle,
};
use sol_system::{
    AppId, NotificationActionId, NotificationActionInvocation, NotificationApi,
    NotificationDismissReason, NotificationError, NotificationId, NotificationLifecycle,
    NotificationQuery, NotificationRecord, NotificationUrgency,
};
use sol_ui::{
    AccessibilityNode, AccessibilityState, SemanticId, SemanticRole, VisualTokenContract,
};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

/// Whether the center shows only active notifications or retained history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NotificationScope {
    /// Present notifications still eligible for transient display.
    #[default]
    Active,
    /// Active and dismissed notifications retained by the notification service.
    History,
}

/// A stable grouping key for center sections.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NotificationGroupKey {
    /// Application attributed by the notification service.
    pub app_id: AppId,
    /// Urgency section within one application's notifications.
    pub urgency: NotificationUrgency,
}

/// One renderer-neutral notification center section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationGroup {
    /// Stable application + urgency section identity.
    pub key: NotificationGroupKey,
    /// Records ordered newest first by service sequence.
    pub records: Vec<NotificationRecord>,
}

/// Keyboard input interpreted by the notification center.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationCenterKey {
    /// Focus the previous visible notification.
    ArrowUp,
    /// Focus the next visible notification.
    ArrowDown,
    /// Invoke the first declared action on the focused notification.
    Enter,
    /// Dismiss the focused active notification as a user action.
    Delete,
    /// Remove keyboard focus from the center.
    Escape,
}

/// Result of a keyboard or typed center operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationCenterOutcome {
    /// Focus moved to a notification.
    FocusChanged(NotificationId),
    /// The focused notification was dismissed through the service.
    Dismissed(NotificationRecord),
    /// The service validated one application action for delivery.
    ActionInvoked(NotificationActionInvocation),
    /// Focus was cleared.
    FocusCleared,
    /// Input had no applicable effect.
    Ignored,
}

/// Error returned by a notification-center model operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationCenterError {
    /// The notification service failed the typed request.
    Service(NotificationError),
    /// A requested ID is not currently represented in the selected scope.
    NotVisible(NotificationId),
    /// The focused notification declares no actions.
    NoAction(NotificationId),
}

impl fmt::Display for NotificationCenterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Service(error) => error.fmt(formatter),
            Self::NotVisible(id) => write!(
                formatter,
                "notification {id} is not visible in this center scope"
            ),
            Self::NoAction(id) => write!(formatter, "notification {id} has no declared actions"),
        }
    }
}

impl Error for NotificationCenterError {}

impl From<NotificationError> for NotificationCenterError {
    fn from(error: NotificationError) -> Self {
        Self::Service(error)
    }
}

/// Result returned by notification-center operations.
pub type NotificationCenterResult<T> = Result<T, NotificationCenterError>;

/// Stable, renderer-neutral notification center state.
pub struct NotificationCenter<A: NotificationApi> {
    api: A,
    scope: NotificationScope,
    records: Vec<NotificationRecord>,
    focused: Option<NotificationId>,
}

impl<A: NotificationApi> NotificationCenter<A> {
    /// Create an empty active-notification center backed by `api`.
    #[must_use]
    pub fn new(api: A) -> Self {
        Self {
            api,
            scope: NotificationScope::Active,
            records: Vec::new(),
            focused: None,
        }
    }

    /// Return the selected service query scope.
    #[must_use]
    pub const fn scope(&self) -> NotificationScope {
        self.scope
    }

    /// Return the notification ID currently selected by keyboard navigation.
    #[must_use]
    pub const fn focused(&self) -> Option<NotificationId> {
        self.focused
    }

    /// Return the current service projection ordered newest first.
    #[must_use]
    pub fn records(&self) -> &[NotificationRecord] {
        &self.records
    }

    /// Select active-only or retained-history query scope and refresh it.
    pub fn set_scope(&mut self, scope: NotificationScope) -> NotificationCenterResult<()> {
        self.scope = scope;
        self.refresh()
    }

    /// Fetch the current typed notification service projection.
    pub fn refresh(&mut self) -> NotificationCenterResult<()> {
        let query = match self.scope {
            NotificationScope::Active => NotificationQuery::Active,
            NotificationScope::History => NotificationQuery::History,
        };
        self.records = self.api.query(query)?;
        if self
            .focused
            .is_some_and(|id| !self.records.iter().any(|record| record.id == id))
        {
            self.focused = None;
        }
        Ok(())
    }

    /// Group records by application and urgency for a center renderer.
    #[must_use]
    pub fn groups(&self) -> Vec<NotificationGroup> {
        let mut grouped: BTreeMap<NotificationGroupKey, Vec<NotificationRecord>> = BTreeMap::new();
        for record in &self.records {
            grouped
                .entry(NotificationGroupKey {
                    app_id: record.notification.app_id.clone(),
                    urgency: record.notification.urgency,
                })
                .or_default()
                .push(record.clone());
        }
        let mut groups: Vec<_> = grouped
            .into_iter()
            .map(|(key, mut records)| {
                records.sort_by_key(|record| std::cmp::Reverse(record.sequence));
                NotificationGroup { key, records }
            })
            .collect();
        groups.sort_by(|left, right| {
            urgency_rank(right.key.urgency)
                .cmp(&urgency_rank(left.key.urgency))
                .then_with(|| left.key.app_id.cmp(&right.key.app_id))
        });
        groups
    }

    /// Dismiss a visible active notification through the typed service.
    pub fn dismiss(&mut self, id: NotificationId) -> NotificationCenterResult<NotificationRecord> {
        self.require_visible(id)?;
        let dismissed = self.api.dismiss(id, NotificationDismissReason::User)?;
        self.refresh()?;
        Ok(dismissed)
    }

    /// Invoke an explicitly selected action through the typed service.
    pub fn invoke_action(
        &mut self,
        id: NotificationId,
        action_id: NotificationActionId,
    ) -> NotificationCenterResult<NotificationActionInvocation> {
        self.require_visible(id)?;
        Ok(self.api.invoke_action(id, action_id)?)
    }

    /// Interpret one normalized keyboard event.
    pub fn handle_key(
        &mut self,
        key: NotificationCenterKey,
    ) -> NotificationCenterResult<NotificationCenterOutcome> {
        match key {
            NotificationCenterKey::ArrowUp => Ok(self.move_focus(true)),
            NotificationCenterKey::ArrowDown => Ok(self.move_focus(false)),
            NotificationCenterKey::Delete => {
                let Some(id) = self.focused else {
                    return Ok(NotificationCenterOutcome::Ignored);
                };
                Ok(NotificationCenterOutcome::Dismissed(self.dismiss(id)?))
            }
            NotificationCenterKey::Enter => {
                let Some(id) = self.focused else {
                    return Ok(NotificationCenterOutcome::Ignored);
                };
                let action = self
                    .record(id)?
                    .notification
                    .actions
                    .first()
                    .map(|action| action.id.clone())
                    .ok_or(NotificationCenterError::NoAction(id))?;
                Ok(NotificationCenterOutcome::ActionInvoked(
                    self.invoke_action(id, action)?,
                ))
            }
            NotificationCenterKey::Escape => {
                self.focused = None;
                Ok(NotificationCenterOutcome::FocusCleared)
            }
        }
    }

    /// Build a renderer-independent accessibility tree for the current projection.
    #[must_use]
    pub fn accessibility_tree(&self) -> AccessibilityNode {
        AccessibilityNode {
            id: SemanticId::new("notification-center"),
            role: SemanticRole::Group,
            label: "Notification Center".to_owned(),
            value: Some(match self.scope {
                NotificationScope::Active => "active notifications".to_owned(),
                NotificationScope::History => "notification history".to_owned(),
            }),
            state: AccessibilityState::default(),
            children: self
                .groups()
                .into_iter()
                .map(|group| AccessibilityNode {
                    id: SemanticId::new(format!(
                        "notification-group-{}-{:?}",
                        group.key.app_id.as_str(),
                        group.key.urgency
                    )),
                    role: SemanticRole::Group,
                    label: format!("{} {:?}", group.key.app_id, group.key.urgency),
                    value: Some(format!("{} notifications", group.records.len())),
                    state: AccessibilityState::default(),
                    children: group
                        .records
                        .into_iter()
                        .map(|record| AccessibilityNode {
                            id: SemanticId::new(format!("notification-{}", record.id.get())),
                            role: SemanticRole::Button,
                            label: record.notification.summary.clone(),
                            value: record.notification.body.clone(),
                            state: AccessibilityState {
                                focused: self.focused == Some(record.id),
                                selected: record.lifecycle == NotificationLifecycle::Active,
                                disabled: record.lifecycle != NotificationLifecycle::Active,
                                ..AccessibilityState::default()
                            },
                            children: record
                                .notification
                                .actions
                                .into_iter()
                                .map(|action| AccessibilityNode {
                                    id: SemanticId::new(format!(
                                        "notification-action-{}-{}",
                                        record.id.get(),
                                        action.id.as_str()
                                    )),
                                    role: SemanticRole::Button,
                                    label: action.label,
                                    value: Some(action.id.as_str().to_owned()),
                                    state: AccessibilityState {
                                        disabled: record.lifecycle != NotificationLifecycle::Active,
                                        ..AccessibilityState::default()
                                    },
                                    children: Vec::new(),
                                })
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    /// Return a complete token-only visual contract for a center surface.
    #[must_use]
    pub const fn visual_tokens(&self) -> VisualTokenContract {
        VisualTokenContract {
            background: Color::Elevated,
            foreground: Color::TextPrimary,
            padding: Spacing::Lg,
            radius: Radius::Md,
            metric: ControlMetric::Toolbar,
            motion: Motion::Panel,
            typography: FontStyle::Body,
        }
    }

    fn move_focus(&mut self, previous: bool) -> NotificationCenterOutcome {
        if self.records.is_empty() {
            return NotificationCenterOutcome::Ignored;
        }
        let current = self
            .focused
            .and_then(|id| self.records.iter().position(|record| record.id == id));
        let index = match current {
            Some(index) if previous => (index + self.records.len() - 1) % self.records.len(),
            Some(index) => (index + 1) % self.records.len(),
            None if previous => self.records.len() - 1,
            None => 0,
        };
        let id = self.records[index].id;
        self.focused = Some(id);
        NotificationCenterOutcome::FocusChanged(id)
    }

    fn require_visible(&self, id: NotificationId) -> NotificationCenterResult<()> {
        self.record(id).map(|_| ())
    }

    fn record(&self, id: NotificationId) -> NotificationCenterResult<&NotificationRecord> {
        self.records
            .iter()
            .find(|record| record.id == id)
            .ok_or(NotificationCenterError::NotVisible(id))
    }
}

const fn urgency_rank(urgency: NotificationUrgency) -> u8 {
    match urgency {
        NotificationUrgency::Low => 0,
        NotificationUrgency::Normal => 1,
        NotificationUrgency::Critical => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sol_system::{
        NotificationAction, NotificationRequest, NotificationResult, NotificationUrgency,
    };
    use std::sync::Mutex;

    #[derive(Debug)]
    struct FixtureNotificationAdapter {
        records: Mutex<Vec<NotificationRecord>>,
    }

    impl FixtureNotificationAdapter {
        fn new(records: Vec<NotificationRecord>) -> Self {
            Self {
                records: Mutex::new(records),
            }
        }
    }

    impl NotificationApi for FixtureNotificationAdapter {
        fn publish(&self, _request: NotificationRequest) -> NotificationResult<NotificationRecord> {
            Err(NotificationError::backend(
                "fixture only models center reads",
            ))
        }

        fn dismiss(
            &self,
            id: NotificationId,
            reason: NotificationDismissReason,
        ) -> NotificationResult<NotificationRecord> {
            let mut records = self
                .records
                .lock()
                .map_err(|error| NotificationError::backend(error.to_string()))?;
            let record = records
                .iter_mut()
                .find(|record| record.id == id)
                .ok_or(NotificationError::NotFound(id))?;
            record.lifecycle = NotificationLifecycle::Dismissed(reason);
            record.sequence += 1;
            Ok(record.clone())
        }

        fn query(&self, query: NotificationQuery) -> NotificationResult<Vec<NotificationRecord>> {
            let records = self
                .records
                .lock()
                .map_err(|error| NotificationError::backend(error.to_string()))?;
            let mut result: Vec<_> = records
                .iter()
                .filter(|record| match query {
                    NotificationQuery::Active => record.lifecycle == NotificationLifecycle::Active,
                    NotificationQuery::History => true,
                    NotificationQuery::ForApp { .. } => false,
                })
                .cloned()
                .collect();
            result.sort_by_key(|record| std::cmp::Reverse(record.sequence));
            Ok(result)
        }

        fn invoke_action(
            &self,
            id: NotificationId,
            action_id: NotificationActionId,
        ) -> NotificationResult<NotificationActionInvocation> {
            let records = self
                .records
                .lock()
                .map_err(|error| NotificationError::backend(error.to_string()))?;
            let record = records
                .iter()
                .find(|record| record.id == id)
                .ok_or(NotificationError::NotFound(id))?;
            if record.lifecycle != NotificationLifecycle::Active {
                return Err(NotificationError::NotActive(id));
            }
            if !record
                .notification
                .actions
                .iter()
                .any(|action| action.id == action_id)
            {
                return Err(NotificationError::UnknownAction(action_id));
            }
            Ok(NotificationActionInvocation {
                notification_id: id,
                app_id: record.notification.app_id.clone(),
                action_id,
            })
        }
    }

    fn action(id: &str, label: &str) -> NotificationAction {
        NotificationAction::new(NotificationActionId::new(id).unwrap(), label).unwrap()
    }

    fn record(
        id: u64,
        app_id: &str,
        urgency: NotificationUrgency,
        actions: Vec<NotificationAction>,
        sequence: u64,
    ) -> NotificationRecord {
        NotificationRecord {
            id: NotificationId::from_raw(id),
            notification: NotificationRequest::new(
                AppId::parse(app_id).unwrap(),
                format!("Notice {id}"),
            )
            .unwrap()
            .with_urgency(urgency)
            .with_actions(actions)
            .unwrap(),
            lifecycle: NotificationLifecycle::Active,
            sequence,
        }
    }

    #[test]
    fn center_groups_urgency_and_exposes_semantic_actions() {
        let api = FixtureNotificationAdapter::new(vec![
            record(1, "org.sol.files", NotificationUrgency::Low, vec![], 1),
            record(
                2,
                "org.sol.files",
                NotificationUrgency::Critical,
                vec![action("open", "Open")],
                3,
            ),
            record(
                3,
                "org.sol.settings",
                NotificationUrgency::Normal,
                vec![],
                2,
            ),
        ]);
        let mut center = NotificationCenter::new(api);
        center.refresh().unwrap();
        let groups = center.groups();
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].key.urgency, NotificationUrgency::Critical);
        assert_eq!(center.accessibility_tree().children.len(), 3);
        assert_eq!(center.visual_tokens().motion, Motion::Panel);
        assert_eq!(
            center.handle_key(NotificationCenterKey::ArrowDown).unwrap(),
            NotificationCenterOutcome::FocusChanged(NotificationId::from_raw(2))
        );
        assert!(matches!(
            center.handle_key(NotificationCenterKey::Enter).unwrap(),
            NotificationCenterOutcome::ActionInvoked(NotificationActionInvocation { .. })
        ));
    }

    #[test]
    fn center_dismisses_through_adapter_and_history_retains_lifecycle() {
        let api = FixtureNotificationAdapter::new(vec![record(
            4,
            "org.sol.files",
            NotificationUrgency::Normal,
            vec![action("open", "Open")],
            4,
        )]);
        let mut center = NotificationCenter::new(api);
        center.refresh().unwrap();
        center.handle_key(NotificationCenterKey::ArrowDown).unwrap();
        assert!(matches!(
            center.handle_key(NotificationCenterKey::Delete).unwrap(),
            NotificationCenterOutcome::Dismissed(NotificationRecord {
                lifecycle: NotificationLifecycle::Dismissed(NotificationDismissReason::User),
                ..
            })
        ));
        assert!(center.records().is_empty());
        center.set_scope(NotificationScope::History).unwrap();
        assert_eq!(center.records().len(), 1);
        assert!(matches!(
            center.handle_key(NotificationCenterKey::Enter),
            Ok(NotificationCenterOutcome::Ignored)
        ));
    }
}
