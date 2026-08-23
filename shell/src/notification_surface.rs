//! Native notification surface projection.
//!
//! The notification service and [`crate::notification_center::NotificationCenter`]
//! own lifecycle and action policy.  This module is the shell boundary that
//! turns that state into one bounded, keyboard-interactive layer-shell frame.
//! Wayland objects stay behind [`NotificationSurfaceHost`], which keeps the
//! contract deterministic for headless tests and for the eventual native host.

use std::time::Duration;

use sol_design::{
    accessibility::TokenMode,
    color::Color,
    metrics::ControlMetric,
    motion::{Motion, MotionSpec},
    radius::Radius,
    spacing::Spacing,
    typography::FontStyle,
};
use sol_system::{
    NotificationActionId, NotificationActionInvocation, NotificationApi, NotificationError,
    NotificationId, NotificationLifecycle, NotificationUrgency,
};
use sol_ui::{AccessibilityNode, AccessibilityState, LogicalSize, SemanticId, SemanticRole};

use crate::{
    notification_center::{
        NotificationCenter, NotificationCenterError, NotificationCenterKey,
        NotificationCenterOutcome, NotificationScope,
    },
    overlay::{Anchor, ExclusiveZone, InputRegion, LayerShellLayer, LogicalPoint},
};

/// Output facts negotiated with the compositor before presenting a surface.
#[derive(Debug, Clone, PartialEq)]
pub struct NotificationOutput {
    /// Stable compositor output identity.
    pub id: String,
    /// Logical output extent.
    pub logical_size: LogicalSize,
    /// Current fractional output scale.
    pub scale_factor: f32,
}

impl NotificationOutput {
    /// Validate output identity, extent, and scale at the native boundary.
    pub fn new(
        id: impl Into<String>,
        logical_size: LogicalSize,
        scale_factor: f32,
    ) -> Result<Self, NotificationSurfaceError> {
        let id = id.into();
        if id.trim().is_empty()
            || !logical_size.width.is_finite()
            || !logical_size.height.is_finite()
            || logical_size.width <= 0.0
            || logical_size.height <= 0.0
            || !scale_factor.is_finite()
            || scale_factor <= 0.0
        {
            return Err(NotificationSurfaceError::InvalidOutput);
        }
        Ok(Self {
            id,
            logical_size,
            scale_factor,
        })
    }
}

/// Presentation policy supplied by Settings or the desktop session.
///
/// A timed notification is not dismissed by this model.  Its duration is
/// emitted in the frame contract so the native host can schedule expiry and
/// call [`NotificationSurface::expire`] through the typed service boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationLifetime {
    /// Keep the card until the user or application dismisses it.
    Sticky,
    /// Ask the host to expire the card after this duration.
    Timed(Duration),
}

/// User policy for transient notification presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotificationPolicy {
    /// Maximum number of cards presented in one frame.
    pub max_visible: usize,
    /// Presentation lifetime for low urgency records.
    pub low: NotificationLifetime,
    /// Presentation lifetime for normal urgency records.
    pub normal: NotificationLifetime,
    /// Presentation lifetime for critical urgency records.
    pub critical: NotificationLifetime,
}

impl Default for NotificationPolicy {
    fn default() -> Self {
        Self {
            max_visible: 4,
            low: NotificationLifetime::Timed(Duration::from_secs(5)),
            normal: NotificationLifetime::Timed(Duration::from_secs(8)),
            critical: NotificationLifetime::Sticky,
        }
    }
}

impl NotificationPolicy {
    /// Validate a policy before it reaches a native host.
    pub fn validate(self) -> Result<Self, NotificationSurfaceError> {
        if self.max_visible == 0 {
            return Err(NotificationSurfaceError::InvalidPolicy(
                "max_visible must be greater than zero",
            ));
        }
        for lifetime in [self.low, self.normal, self.critical] {
            if let NotificationLifetime::Timed(duration) = lifetime {
                if duration.is_zero() {
                    return Err(NotificationSurfaceError::InvalidPolicy(
                        "timed lifetimes must be non-zero",
                    ));
                }
            }
        }
        Ok(self)
    }

    /// Return the host expiry policy for one urgency class.
    #[must_use]
    pub const fn lifetime(self, urgency: NotificationUrgency) -> NotificationLifetime {
        match urgency {
            NotificationUrgency::Low => self.low,
            NotificationUrgency::Normal => self.normal,
            NotificationUrgency::Critical => self.critical,
        }
    }
}

/// One notification card in the native frame.
#[derive(Debug, Clone, PartialEq)]
pub struct NotificationCard {
    /// Daemon-assigned notification identity.
    pub id: NotificationId,
    /// Attributed application identity.
    pub app_id: String,
    /// User-visible title and body.
    pub summary: String,
    pub body: Option<String>,
    /// Priority and lifecycle state from the service.
    pub urgency: NotificationUrgency,
    pub lifecycle: NotificationLifecycle,
    /// Logical card rectangle `(x, y, width, height)`.
    pub rect: (f32, f32, f32, f32),
    /// Keyboard focus state.
    pub focused: bool,
    /// Host expiry policy. `None` means sticky.
    pub dismiss_after: Option<Duration>,
    /// Actions projected into the card.
    pub actions: Vec<NotificationActionId>,
}

/// Complete layer-shell contract for one notification frame.
#[derive(Debug, Clone)]
pub struct NotificationSurfaceContract {
    /// Output and physical frame extent.
    pub output: NotificationOutput,
    pub physical_size: (u32, u32),
    /// Native layer-shell placement policy.
    pub layer: LayerShellLayer,
    pub anchor: Anchor,
    pub exclusive_zone: ExclusiveZone,
    pub input_region: InputRegion,
    pub logical_origin: LogicalPoint,
    /// Token-resolved surface roles.
    pub background: Color,
    pub foreground: Color,
    pub border: Color,
    pub accent: Color,
    pub radius: Radius,
    pub padding: Spacing,
    pub typography: FontStyle,
    pub transition: MotionSpec,
    pub token_mode: TokenMode,
    /// Visible records and the projected accessibility tree.
    pub cards: Vec<NotificationCard>,
    pub accessibility: AccessibilityNode,
}

/// Errors returned before a native host sees an invalid notification frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationSurfaceError {
    /// Output identity, size, or scale is malformed.
    InvalidOutput,
    /// User policy is malformed or unsafe.
    InvalidPolicy(&'static str),
    /// Notification service rejected a typed operation.
    Service(NotificationError),
    /// Notification center rejected a keyboard operation.
    Center(NotificationCenterError),
    /// Application action delivery failed after service validation.
    ActionDelivery(String),
}

impl std::fmt::Display for NotificationSurfaceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidOutput => formatter.write_str("invalid notification output contract"),
            Self::InvalidPolicy(message) => {
                write!(formatter, "invalid notification policy: {message}")
            }
            Self::Service(error) => error.fmt(formatter),
            Self::Center(error) => error.fmt(formatter),
            Self::ActionDelivery(error) => {
                write!(formatter, "notification action delivery failed: {error}")
            }
        }
    }
}

impl std::error::Error for NotificationSurfaceError {}

impl From<NotificationError> for NotificationSurfaceError {
    fn from(error: NotificationError) -> Self {
        Self::Service(error)
    }
}

impl From<NotificationCenterError> for NotificationSurfaceError {
    fn from(error: NotificationCenterError) -> Self {
        Self::Center(error)
    }
}

/// Native host boundary implemented by the Wayland layer-shell adapter.
pub trait NotificationSurfaceHost {
    /// Present a frame and its premultiplied RGBA pixels.
    fn present(&mut self, contract: &NotificationSurfaceContract, pixels: &[u8]);
    /// Destroy or hide the transient layer surface.
    fn dismiss(&mut self);
    /// Transfer keyboard focus to the notification surface.
    fn set_keyboard_focus(&mut self, focused: bool);
}

/// Application callback boundary.  The service validates ownership and action
/// identity first; only then does the shell deliver this typed invocation.
pub trait NotificationActionSink {
    /// Deliver an invocation to the attributed application adapter.
    fn deliver(&mut self, invocation: NotificationActionInvocation) -> Result<(), String>;
}

/// Retained native notification surface state.
pub struct NotificationSurface<A: NotificationApi> {
    center: NotificationCenter<A>,
    output: NotificationOutput,
    policy: NotificationPolicy,
    mode: TokenMode,
    visible: bool,
    /// Last frame retained for a native host and deterministic inspection.
    pub last_contract: Option<NotificationSurfaceContract>,
}

impl<A: NotificationApi> NotificationSurface<A> {
    /// Create a notification surface over a typed notification service.
    pub fn new(
        api: A,
        output: NotificationOutput,
        policy: NotificationPolicy,
        mode: TokenMode,
    ) -> Result<Self, NotificationSurfaceError> {
        Ok(Self {
            center: NotificationCenter::new(api),
            output,
            policy: policy.validate()?,
            mode,
            visible: false,
            last_contract: None,
        })
    }

    /// Read the center's current projection.
    #[must_use]
    pub fn center(&self) -> &NotificationCenter<A> {
        &self.center
    }

    /// Open the transient surface, query active records, and present a frame.
    pub fn open(
        &mut self,
        host: &mut impl NotificationSurfaceHost,
    ) -> Result<(), NotificationSurfaceError> {
        self.center.set_scope(NotificationScope::Active)?;
        self.visible = true;
        host.set_keyboard_focus(true);
        self.present(host)
    }

    /// Close the surface and release keyboard focus without mutating history.
    pub fn close(&mut self, host: &mut impl NotificationSurfaceHost) {
        self.visible = false;
        host.set_keyboard_focus(false);
        host.dismiss();
    }

    /// Refresh active state after service events and repaint if visible.
    pub fn refresh(
        &mut self,
        host: &mut impl NotificationSurfaceHost,
    ) -> Result<(), NotificationSurfaceError> {
        self.center.refresh()?;
        self.present(host)
    }

    /// Expire a card through the same typed dismissal path as user actions.
    pub fn expire(
        &mut self,
        id: NotificationId,
        host: &mut impl NotificationSurfaceHost,
    ) -> Result<(), NotificationSurfaceError> {
        self.center.dismiss(id)?;
        self.present(host)
    }

    /// Route keyboard input and deliver validated actions to the app adapter.
    pub fn handle_key(
        &mut self,
        key: NotificationCenterKey,
        host: &mut impl NotificationSurfaceHost,
        sink: &mut impl NotificationActionSink,
    ) -> Result<NotificationCenterOutcome, NotificationSurfaceError> {
        let outcome = self.center.handle_key(key)?;
        if let NotificationCenterOutcome::ActionInvoked(invocation) = &outcome {
            sink.deliver(invocation.clone())
                .map_err(NotificationSurfaceError::ActionDelivery)?;
        }
        if matches!(outcome, NotificationCenterOutcome::Dismissed(_)) {
            self.present(host)?;
        } else if !matches!(outcome, NotificationCenterOutcome::Ignored) {
            self.present(host)?;
        }
        Ok(outcome)
    }

    /// Build the current token-resolved frame.
    pub fn contract(&self) -> NotificationSurfaceContract {
        let padding = Spacing::Lg;
        let gap = Spacing::Sm.px();
        let width = (ControlMetric::Button.spec().min_width * 4.0)
            .min((self.output.logical_size.width - Spacing::Xl.px() * 2.0).max(1.0));
        let mut cards = Vec::new();
        let records = self
            .center
            .records()
            .iter()
            .filter(|record| record.lifecycle == NotificationLifecycle::Active)
            .take(self.policy.max_visible);
        let title_height = self.mode.typography(FontStyle::Title).pixels;
        for (index, record) in records.enumerate() {
            let body_height = record
                .notification
                .body
                .as_ref()
                .map_or(0.0, |_| self.mode.typography(FontStyle::Body).pixels + gap);
            let action_height = if record.notification.actions.is_empty() {
                0.0
            } else {
                ControlMetric::Button.spec().height + gap
            };
            let height = padding.px() * 2.0 + title_height + body_height + action_height;
            let y = Spacing::Xl.px()
                + cards
                    .iter()
                    .map(|card: &NotificationCard| card.rect.3 + gap)
                    .sum::<f32>();
            cards.push(NotificationCard {
                id: record.id,
                app_id: record.notification.app_id.to_string(),
                summary: record.notification.summary.clone(),
                body: record.notification.body.clone(),
                urgency: record.notification.urgency,
                lifecycle: record.lifecycle,
                rect: (
                    self.output.logical_size.width - Spacing::Xl.px() - width,
                    y,
                    width,
                    height,
                ),
                focused: self.center.focused() == Some(record.id),
                dismiss_after: match self.policy.lifetime(record.notification.urgency) {
                    NotificationLifetime::Timed(duration) => Some(duration),
                    NotificationLifetime::Sticky => None,
                },
                actions: record
                    .notification
                    .actions
                    .iter()
                    .map(|action| action.id.clone())
                    .collect(),
            });
            let _ = index;
        }
        let logical_size = LogicalSize::new(
            width + Spacing::Xl.px() * 2.0,
            self.output.logical_size.height,
        );
        let origin = LogicalPoint {
            x: self.output.logical_size.width - logical_size.width,
            y: 0.0,
        };
        NotificationSurfaceContract {
            output: self.output.clone(),
            physical_size: logical_size.physical_pixels(self.output.scale_factor),
            layer: LayerShellLayer::Overlay,
            anchor: Anchor::TopRight,
            exclusive_zone: ExclusiveZone::None,
            input_region: InputRegion::Interactive,
            logical_origin: origin,
            background: Color::Surface,
            foreground: Color::TextPrimary,
            border: Color::Border,
            accent: Color::Accent,
            radius: Radius::Md,
            padding,
            typography: FontStyle::Body,
            transition: self.mode.motion_spec(Motion::Panel),
            token_mode: self.mode,
            accessibility: accessibility_tree(&cards),
            cards,
        }
    }

    fn present(
        &mut self,
        host: &mut impl NotificationSurfaceHost,
    ) -> Result<(), NotificationSurfaceError> {
        if !self.visible {
            return Ok(());
        }
        let contract = self.contract();
        let pixels = rasterize(&contract);
        host.present(&contract, &pixels);
        self.last_contract = Some(contract);
        Ok(())
    }
}

fn accessibility_tree(cards: &[NotificationCard]) -> AccessibilityNode {
    AccessibilityNode {
        id: SemanticId::new("notification-surface"),
        role: SemanticRole::Group,
        label: "Notifications".to_owned(),
        value: Some(format!("{} visible notifications", cards.len())),
        state: AccessibilityState::default(),
        children: cards
            .iter()
            .map(|card| AccessibilityNode {
                id: SemanticId::new(format!("notification-surface.{}", card.id.get())),
                role: SemanticRole::Button,
                label: card.summary.clone(),
                value: card.body.clone(),
                state: AccessibilityState {
                    focused: card.focused,
                    selected: true,
                    disabled: false,
                    editable: false,
                },
                children: card
                    .actions
                    .iter()
                    .map(|action| AccessibilityNode {
                        id: SemanticId::new(format!(
                            "notification-surface.{}.action.{}",
                            card.id.get(),
                            action.as_str()
                        )),
                        role: SemanticRole::Button,
                        label: action.as_str().to_owned(),
                        value: None,
                        state: AccessibilityState::default(),
                        children: Vec::new(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn rasterize(contract: &NotificationSurfaceContract) -> Vec<u8> {
    let (width, height) = contract.physical_size;
    let mut pixels = vec![0; width as usize * height as usize * 4];
    fill(&mut pixels, contract.background.rgba());
    for card in &contract.cards {
        fill_rect(
            &mut pixels,
            width,
            height,
            card.rect,
            if card.focused {
                contract.accent.rgba()
            } else {
                contract.border.rgba()
            },
            contract.output.scale_factor,
        );
    }
    pixels
}

fn fill(pixels: &mut [u8], color: sol_design::color::Rgba) {
    let value = [
        (color.0 * 255.0) as u8,
        (color.1 * 255.0) as u8,
        (color.2 * 255.0) as u8,
        (color.3 * 255.0) as u8,
    ];
    for chunk in pixels.chunks_exact_mut(4) {
        chunk.copy_from_slice(&value);
    }
}

fn fill_rect(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    rect: (f32, f32, f32, f32),
    color: sol_design::color::Rgba,
    scale: f32,
) {
    let left = (rect.0 * scale).max(0.0) as u32;
    let top = (rect.1 * scale).max(0.0) as u32;
    let right = ((rect.0 + rect.2) * scale).min(width as f32).max(0.0) as u32;
    let bottom = ((rect.1 + rect.3) * scale).min(height as f32).max(0.0) as u32;
    let value = [
        (color.0 * 255.0) as u8,
        (color.1 * 255.0) as u8,
        (color.2 * 255.0) as u8,
        (color.3 * 255.0) as u8,
    ];
    for y in top..bottom {
        for x in left..right {
            let index = ((y * width + x) * 4) as usize;
            pixels[index..index + 4].copy_from_slice(&value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sol_system::{
        AppId, NotificationAction, NotificationDismissReason, NotificationQuery,
        NotificationRecord, NotificationRequest, NotificationResult,
    };
    use std::sync::Mutex;

    #[derive(Debug)]
    struct FixtureApi {
        records: Mutex<Vec<NotificationRecord>>,
    }

    impl NotificationApi for FixtureApi {
        fn publish(&self, _request: NotificationRequest) -> NotificationResult<NotificationRecord> {
            Err(NotificationError::backend("fixture does not publish"))
        }
        fn dismiss(
            &self,
            id: NotificationId,
            reason: NotificationDismissReason,
        ) -> NotificationResult<NotificationRecord> {
            let mut records = self.records.lock().unwrap();
            let record = records
                .iter_mut()
                .find(|record| record.id == id)
                .ok_or(NotificationError::NotFound(id))?;
            record.lifecycle = NotificationLifecycle::Dismissed(reason);
            Ok(record.clone())
        }
        fn query(&self, query: NotificationQuery) -> NotificationResult<Vec<NotificationRecord>> {
            let records = self.records.lock().unwrap();
            Ok(records
                .iter()
                .filter(|record| match query {
                    NotificationQuery::Active => record.lifecycle == NotificationLifecycle::Active,
                    NotificationQuery::History => true,
                    NotificationQuery::ForApp { .. } => true,
                })
                .cloned()
                .collect())
        }
        fn invoke_action(
            &self,
            id: NotificationId,
            action_id: NotificationActionId,
        ) -> NotificationResult<NotificationActionInvocation> {
            let records = self.records.lock().unwrap();
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

    #[derive(Default)]
    struct Host {
        presented: Vec<(NotificationSurfaceContract, usize)>,
        focused: Vec<bool>,
        dismissed: usize,
    }
    impl NotificationSurfaceHost for Host {
        fn present(&mut self, contract: &NotificationSurfaceContract, pixels: &[u8]) {
            self.presented.push((contract.clone(), pixels.len()));
        }
        fn dismiss(&mut self) {
            self.dismissed += 1;
        }
        fn set_keyboard_focus(&mut self, focused: bool) {
            self.focused.push(focused);
        }
    }

    #[derive(Default)]
    struct Sink {
        invocations: Vec<NotificationActionInvocation>,
    }
    impl NotificationActionSink for Sink {
        fn deliver(&mut self, invocation: NotificationActionInvocation) -> Result<(), String> {
            self.invocations.push(invocation);
            Ok(())
        }
    }

    fn record(id: u64, with_action: bool) -> NotificationRecord {
        let mut request = NotificationRequest::new(
            AppId::parse("org.sol.files").unwrap(),
            format!("Notice {id}"),
        )
        .unwrap()
        .with_body("Body");
        if with_action {
            request = request
                .with_actions(vec![
                    NotificationAction::new(NotificationActionId::new("open").unwrap(), "Open")
                        .unwrap(),
                ])
                .unwrap();
        }
        NotificationRecord {
            id: NotificationId::from_raw(id),
            notification: request,
            lifecycle: NotificationLifecycle::Active,
            sequence: id,
        }
    }

    fn surface() -> NotificationSurface<FixtureApi> {
        NotificationSurface::new(
            FixtureApi {
                records: Mutex::new(vec![record(1, true), record(2, false)]),
            },
            NotificationOutput::new("eDP-1", LogicalSize::new(800.0, 600.0), 1.25).unwrap(),
            NotificationPolicy::default(),
            TokenMode::dark(),
        )
        .unwrap()
    }

    #[test]
    fn native_surface_projects_scaled_cards_and_policy() {
        let mut surface = surface();
        let mut host = Host::default();
        surface.open(&mut host).unwrap();
        assert_eq!(host.presented.len(), 1);
        assert_eq!(host.presented[0].0.layer, LayerShellLayer::Overlay);
        assert_eq!(host.presented[0].0.anchor, Anchor::TopRight);
        assert_eq!(host.presented[0].0.physical_size.1, 750);
        assert_eq!(
            host.presented[0].1,
            host.presented[0].0.physical_size.0 as usize * 750 * 4
        );
        assert_eq!(host.presented[0].0.cards.len(), 2);
        assert_eq!(host.presented[0].0.accessibility.children.len(), 2);
        assert_eq!(
            host.presented[0].0.cards[0].dismiss_after,
            Some(Duration::from_secs(8))
        );
    }

    #[test]
    fn keyboard_action_is_delivered_only_after_service_validation() {
        let mut surface = surface();
        let mut host = Host::default();
        let mut sink = Sink::default();
        surface.open(&mut host).unwrap();
        surface
            .handle_key(NotificationCenterKey::ArrowDown, &mut host, &mut sink)
            .unwrap();
        let result = surface
            .handle_key(NotificationCenterKey::Enter, &mut host, &mut sink)
            .unwrap();
        assert!(matches!(
            result,
            NotificationCenterOutcome::ActionInvoked(_)
        ));
        assert_eq!(sink.invocations.len(), 1);
        surface
            .handle_key(NotificationCenterKey::Delete, &mut host, &mut sink)
            .unwrap();
        assert_eq!(surface.center().records().len(), 1);
    }

    #[test]
    fn invalid_policy_and_output_are_rejected() {
        assert!(
            NotificationPolicy {
                max_visible: 0,
                ..NotificationPolicy::default()
            }
            .validate()
            .is_err()
        );
        assert_eq!(
            NotificationOutput::new("", LogicalSize::new(1.0, 1.0), 1.0),
            Err(NotificationSurfaceError::InvalidOutput)
        );
    }
}
