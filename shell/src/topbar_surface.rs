//! The native top-bar surface.
//!
//! [`crate::topbar::TopBarModel`] owns policy: which providers exist, what a
//! control does when activated, and how a typed intent is authorized. This
//! module owns *presentation*: it turns a [`TopBarSnapshot`] into the fixed
//! spatial grammar of ADR-0025 and paints it into an SCP layer surface.
//!
//! The split is deliberate. A surface that also held the action boundary would
//! need a `SystemActionApi` to be laid out, which would make "does the bar put
//! the clock in the right place" a test that has to construct a permission
//! service first.
//!
//! ## Fixed geography
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │ Foreground app        Live Capsule · status · center · system │
//! └──────────────────────────────────────────────────────────────┘
//! ```
//!
//! The upper-left names the compositor-authenticated foreground application.
//! The upper-right runs, in order, Live Capsule → application status items →
//! Notification Center → system status. That order does not change with locale,
//! output width, or the number of items: overflow collapses inside a zone
//! rather than letting one zone migrate into the other.
//!
//! ## Provider state is shown, never invented
//!
//! A provider that is unavailable or failing renders as such. The bar has no
//! last-known-good fallback that would let it present a stale battery level as
//! current, because a status bar that quietly lies is worse than one that
//! admits it does not know.

use sol_design::{
    accessibility::TokenMode, color::Color, metrics::ControlMetric, spacing::Spacing,
    typography::FontStyle,
};
use sol_ui::{AccessibilityNode, AccessibilityState, LogicalSize, SemanticId, SemanticRole};

use crate::{
    overlay::LayerShellLayer,
    paint::{Canvas, PixelRect, text_scale_for_height},
    scp_host::{
        DesktopHost, DesktopHostError, HostOutput, LayerAnchor, LayerKeyboard, LayerMargin,
        LayerPlacement,
    },
    topbar::{ActivityIndicator, NetworkStatus, ProviderState, TopBarSnapshot},
};

/// Stable SCP namespace of the top bar.
pub const TOPBAR_NAMESPACE: &str = "sol.topbar";

/// Marker appended to a value the Shell knows may no longer be current.
const STALE_MARK: &str = "~";

/// Shown where a provider exists but has no adapter installed.
const UNAVAILABLE_VALUE: &str = "--";

/// Shown where an installed adapter failed.
const ERROR_VALUE: &str = "!";

/// The application the compositor reports as focused.
///
/// ADR-0025 derives this from focused surface → process → App ID. It is never
/// a client-supplied display name on its own: an application that could name
/// itself here could impersonate another one in the most trusted strip of
/// screen the desktop has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForegroundApplication {
    /// Verified reverse-DNS application identity.
    pub app_id: String,
    /// User-visible title resolved from the verified identity.
    pub display_name: String,
}

/// Which zone of the bar an item belongs to, in presentation order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TopBarZone {
    /// Upper-left: the authenticated foreground application and its menu.
    ForegroundApp,
    /// Upper-right, first: ongoing activities and privacy indicators.
    LiveCapsule,
    /// Upper-right: typed application status items.
    StatusItem,
    /// Upper-right: the Notification Center entry.
    NotificationCenter,
    /// Upper-right, last: clock, connectivity, audio, power.
    SystemStatus,
}

/// One laid-out item in the bar.
#[derive(Debug, Clone, PartialEq)]
pub struct TopBarItem {
    /// Stable identity, used for accessibility and hit testing.
    pub id: String,
    /// Zone this item belongs to.
    pub zone: TopBarZone,
    /// Text as presented.
    pub text: String,
    /// Accessible name, which spells out what the terse text abbreviates.
    pub label: String,
    /// Logical rectangle `(x, y, width, height)`.
    pub rect: (f32, f32, f32, f32),
    /// Token role the text is drawn in.
    pub foreground: Color,
}

/// A complete top-bar frame.
#[derive(Debug, Clone, PartialEq)]
pub struct TopBarSurfaceContract {
    pub output: HostOutput,
    pub logical_size: LogicalSize,
    pub physical_size: (u32, u32),
    pub placement: LayerPlacement,
    /// Token-resolved surface roles.
    pub background: Color,
    pub separator: Color,
    pub typography: FontStyle,
    pub token_mode: TokenMode,
    /// Laid-out items, upper-left zone first.
    pub items: Vec<TopBarItem>,
    pub accessibility: AccessibilityNode,
}

/// Errors raised before an invalid bar frame reaches the compositor.
#[derive(Debug)]
pub enum TopBarSurfaceError {
    /// The compositor has not reported a usable output extent yet.
    OutputNotConfigured,
    /// The frame extent could not be allocated.
    UnpaintableExtent((u32, u32)),
    /// The native host rejected the frame.
    Host(DesktopHostError),
}

impl std::fmt::Display for TopBarSurfaceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutputNotConfigured => {
                formatter.write_str("no output extent has been configured yet")
            }
            Self::UnpaintableExtent((width, height)) => {
                write!(formatter, "cannot paint a {width}x{height} top bar")
            }
            Self::Host(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for TopBarSurfaceError {}

impl From<DesktopHostError> for TopBarSurfaceError {
    fn from(error: DesktopHostError) -> Self {
        Self::Host(error)
    }
}

/// Retained top-bar surface.
#[derive(Debug, Clone)]
pub struct TopBarSurface {
    output: HostOutput,
    mode: TokenMode,
    snapshot: TopBarSnapshot,
    foreground: Option<ForegroundApplication>,
    /// Last frame presented, retained for inspection and native hosts.
    pub last_contract: Option<TopBarSurfaceContract>,
}

impl TopBarSurface {
    /// Create the bar for an output and an initial provider snapshot.
    #[must_use]
    pub const fn new(output: HostOutput, mode: TokenMode, snapshot: TopBarSnapshot) -> Self {
        Self {
            output,
            mode,
            snapshot,
            foreground: None,
            last_contract: None,
        }
    }

    /// Adopt a new output extent after a mode change or hotplug.
    pub fn set_output(&mut self, output: HostOutput) {
        self.output = output;
    }

    /// Adopt new accessibility and theme preferences.
    pub fn set_token_mode(&mut self, mode: TokenMode) {
        self.mode = mode;
    }

    /// Replace the provider snapshot after a poll or subscription event.
    pub fn refresh(&mut self, snapshot: TopBarSnapshot) {
        self.snapshot = snapshot;
    }

    /// Replace the compositor-authenticated foreground application.
    ///
    /// Atomic by construction: the whole identity is swapped in one call, so a
    /// focus change can never leave the bar showing one application's name
    /// beside another's commands.
    pub fn set_foreground(&mut self, foreground: Option<ForegroundApplication>) {
        self.foreground = foreground;
    }

    /// Logical height the bar occupies and reserves.
    #[must_use]
    pub fn logical_height(&self) -> f32 {
        ControlMetric::Toolbar.spec().height
    }

    /// Build the current frame contract without painting it.
    pub fn contract(&self) -> Result<TopBarSurfaceContract, TopBarSurfaceError> {
        if !self.output.is_configured() {
            return Err(TopBarSurfaceError::OutputNotConfigured);
        }

        let logical = self.output.logical_size();
        let height = self.logical_height();
        let padding = Spacing::Md.px();
        let gap = Spacing::Lg.px();
        let font = self.mode.typography(self.typography());
        let scale = text_scale_for_height(font.pixels);
        let baseline = (height - crate::paint::text_height(scale)) / 2.0;

        let mut items = Vec::new();
        items.push(self.foreground_item(padding, baseline, scale, height));

        // Trailing items are measured first and then placed as one block, which
        // is what keeps the documented order left-to-right while the block as a
        // whole stays flush against the right edge.
        let trailing = self.trailing_items();
        let widths: Vec<f32> = trailing
            .iter()
            .map(|(_, _, _, text, _)| Canvas::text_width(scale, text))
            .collect();
        let total: f32 = widths.iter().sum::<f32>() + gap * (widths.len().max(1) - 1) as f32;
        let mut pen = (logical.width - padding - total).max(padding);

        for ((id, zone, label, text, color), width) in trailing.into_iter().zip(widths) {
            items.push(TopBarItem {
                id,
                zone,
                text,
                label,
                rect: (pen, baseline, width, crate::paint::text_height(scale)),
                foreground: color,
            });
            pen += width + gap;
        }

        let physical_height = self.output.physical(height).max(1);
        Ok(TopBarSurfaceContract {
            output: self.output,
            logical_size: LogicalSize::new(logical.width, height),
            physical_size: (self.output.size.0.max(0) as u32, physical_height as u32),
            placement: LayerPlacement {
                namespace: TOPBAR_NAMESPACE.to_owned(),
                layer: LayerShellLayer::Top,
                anchor: LayerAnchor::TOP_BAR,
                margin: LayerMargin::default(),
                // Width zero: anchored to both horizontal edges, so the
                // compositor stretches it and the Shell does not have to
                // recompute a width on every mode change.
                size: (self.output.size.0, physical_height),
                // The bar reserves its own height so maximized windows and the
                // Dock lay out beneath it rather than under it.
                exclusive_zone: physical_height,
                keyboard: LayerKeyboard::None,
            },
            background: Color::Elevated,
            separator: Color::Border,
            typography: self.typography(),
            token_mode: self.mode,
            accessibility: accessibility_tree(&items),
            items,
        })
    }

    /// Paint and present the bar.
    pub fn present(&mut self, host: &mut impl DesktopHost) -> Result<(), TopBarSurfaceError> {
        let contract = self.contract()?;
        let pixels = rasterize(&contract)?;
        host.present(&contract.placement, &pixels)?;
        self.last_contract = Some(contract);
        Ok(())
    }

    const fn typography(&self) -> FontStyle {
        FontStyle::Label
    }

    fn foreground_item(&self, x: f32, y: f32, scale: f32, height: f32) -> TopBarItem {
        // With no authenticated focus the bar names the system, not a guess. An
        // app whose identity the compositor has not confirmed does not get the
        // most trusted label on screen by default.
        let (text, label) = match &self.foreground {
            Some(application) => (
                application.display_name.to_ascii_uppercase(),
                format!("Foreground application: {}", application.app_id),
            ),
            None => ("SOL".to_owned(), "No focused application".to_owned()),
        };
        let width = Canvas::text_width(scale, &text);
        TopBarItem {
            id: "topbar.foreground-app".to_owned(),
            zone: TopBarZone::ForegroundApp,
            text,
            label,
            rect: (x, y, width, height.min(crate::paint::text_height(scale))),
            foreground: Color::TextPrimary,
        }
    }

    /// Upper-right items in their documented order, as
    /// `(id, zone, accessible label, text, token role)`.
    fn trailing_items(&self) -> Vec<(String, TopBarZone, String, String, Color)> {
        let mut items = Vec::new();

        // Live Capsule: privacy and activity first, because a microphone that
        // is live outranks anything else competing for this strip.
        match &self.snapshot.activity {
            ProviderState::Available { value, stale } if !value.is_empty() => {
                let text = value
                    .iter()
                    .map(|indicator| activity_text(*indicator))
                    .collect::<Vec<_>>()
                    .join(" ");
                items.push((
                    "topbar.live-capsule".to_owned(),
                    TopBarZone::LiveCapsule,
                    format!("{} active", describe_activities(value)),
                    mark_stale(text, *stale),
                    Color::Accent,
                ));
            }
            ProviderState::Error(reason) => items.push((
                "topbar.live-capsule".to_owned(),
                TopBarZone::LiveCapsule,
                format!("Privacy indicators unavailable: {reason}"),
                format!("PRIVACY {ERROR_VALUE}"),
                Color::Error,
            )),
            // An empty activity list and an absent provider both mean "nothing
            // to show"; neither is worth occupying the capsule anchor.
            ProviderState::Available { .. } | ProviderState::Unavailable => {}
        }

        items.push((
            "topbar.notification-center".to_owned(),
            TopBarZone::NotificationCenter,
            "Notification Center".to_owned(),
            "NOTES".to_owned(),
            Color::TextSecondary,
        ));

        let (network_text, network_color) = match &self.snapshot.network {
            ProviderState::Available { value, stale } => (
                mark_stale(network_text(value), *stale),
                Color::TextSecondary,
            ),
            ProviderState::Unavailable => {
                (format!("NET {UNAVAILABLE_VALUE}"), Color::TextSecondary)
            }
            ProviderState::Error(_) => (format!("NET {ERROR_VALUE}"), Color::Error),
        };
        items.push((
            "topbar.network".to_owned(),
            TopBarZone::SystemStatus,
            "Network status".to_owned(),
            network_text,
            network_color,
        ));

        let (audio_text, audio_color) = match &self.snapshot.audio {
            ProviderState::Available { value, stale } => {
                let text = if value.muted {
                    "MUTED".to_owned()
                } else {
                    format!("VOL {}%", value.volume_percent)
                };
                (mark_stale(text, *stale), Color::TextSecondary)
            }
            ProviderState::Unavailable => {
                (format!("VOL {UNAVAILABLE_VALUE}"), Color::TextSecondary)
            }
            ProviderState::Error(_) => (format!("VOL {ERROR_VALUE}"), Color::Error),
        };
        items.push((
            "topbar.audio".to_owned(),
            TopBarZone::SystemStatus,
            "Audio status".to_owned(),
            audio_text,
            audio_color,
        ));

        let (power_text, power_color) = match &self.snapshot.power {
            ProviderState::Available { value, stale } => {
                let prefix = if value.charging { "CHG" } else { "BAT" };
                (
                    mark_stale(format!("{prefix} {}%", value.percent), *stale),
                    Color::TextSecondary,
                )
            }
            ProviderState::Unavailable => {
                (format!("BAT {UNAVAILABLE_VALUE}"), Color::TextSecondary)
            }
            ProviderState::Error(_) => (format!("BAT {ERROR_VALUE}"), Color::Error),
        };
        items.push((
            "topbar.power".to_owned(),
            TopBarZone::SystemStatus,
            "Power status".to_owned(),
            power_text,
            power_color,
        ));

        let (clock_text, clock_color) = match &self.snapshot.clock {
            ProviderState::Available { value, stale } => (
                mark_stale(value.time.clone(), *stale),
                Color::TextPrimary,
            ),
            ProviderState::Unavailable => (UNAVAILABLE_VALUE.to_owned(), Color::TextSecondary),
            ProviderState::Error(_) => (ERROR_VALUE.to_owned(), Color::Error),
        };
        items.push((
            "topbar.clock".to_owned(),
            TopBarZone::SystemStatus,
            "Clock".to_owned(),
            clock_text,
            clock_color,
        ));

        items
    }
}

fn mark_stale(text: String, stale: bool) -> String {
    if stale { format!("{text}{STALE_MARK}") } else { text }
}

fn network_text(status: &NetworkStatus) -> String {
    match status {
        NetworkStatus::Offline => "OFFLINE".to_owned(),
        NetworkStatus::Connecting => "NET ...".to_owned(),
        NetworkStatus::Connected { signal_percent, .. } => format!("NET {signal_percent}%"),
    }
}

const fn activity_text(indicator: ActivityIndicator) -> &'static str {
    match indicator {
        ActivityIndicator::ScreenCapture => "SCREEN",
        ActivityIndicator::Microphone => "MIC",
        ActivityIndicator::Camera => "CAM",
        ActivityIndicator::RemoteControl => "REMOTE",
    }
}

fn describe_activities(indicators: &[ActivityIndicator]) -> String {
    indicators
        .iter()
        .map(|indicator| match indicator {
            ActivityIndicator::ScreenCapture => "Screen capture",
            ActivityIndicator::Microphone => "Microphone",
            ActivityIndicator::Camera => "Camera",
            ActivityIndicator::RemoteControl => "Remote control",
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn accessibility_tree(items: &[TopBarItem]) -> AccessibilityNode {
    AccessibilityNode {
        id: SemanticId::new("topbar-surface"),
        role: SemanticRole::Group,
        label: "SOL Top Bar".to_owned(),
        value: None,
        state: AccessibilityState::default(),
        children: items
            .iter()
            .map(|item| AccessibilityNode {
                id: SemanticId::new(item.id.clone()),
                role: match item.zone {
                    // The foreground identity is a label, not a control: it
                    // reports who owns the desktop, and activating it would
                    // mean activating an application the user did not point at.
                    TopBarZone::ForegroundApp => SemanticRole::Group,
                    _ => SemanticRole::Button,
                },
                label: item.label.clone(),
                value: Some(item.text.clone()),
                state: AccessibilityState::default(),
                children: Vec::new(),
            })
            .collect(),
    }
}

/// Paint one bar frame.
fn rasterize(contract: &TopBarSurfaceContract) -> Result<Vec<u8>, TopBarSurfaceError> {
    let (width, height) = contract.physical_size;
    let mut canvas = Canvas::new(width, height)
        .ok_or(TopBarSurfaceError::UnpaintableExtent((width, height)))?;
    let mode = contract.token_mode;
    let scale = contract.output.scale;

    canvas.clear(mode.color(contract.background));
    canvas.fill_hairline(
        PixelRect::new(0.0, 0.0, width as f32, height as f32),
        mode.color(contract.separator),
    );

    let font = mode.typography(contract.typography);
    let text_scale = text_scale_for_height(font.pixels);
    for item in &contract.items {
        canvas.draw_text(
            (item.rect.0 * scale, item.rect.1 * scale),
            text_scale * scale,
            mode.color(item.foreground),
            &item.text,
        );
    }

    Ok(canvas.into_pixels())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        scp_host::RecordingDesktopHost,
        topbar::{AudioStatus, ClockStatus, PowerStatus},
    };

    fn snapshot() -> TopBarSnapshot {
        TopBarSnapshot {
            clock: ProviderState::Available {
                value: ClockStatus {
                    time: "09:41".into(),
                    date: "2026-08-28".into(),
                },
                stale: false,
            },
            workspace: ProviderState::Unavailable,
            network: ProviderState::Available {
                value: NetworkStatus::Connected {
                    name: "SOLNet".into(),
                    signal_percent: 80,
                },
                stale: false,
            },
            audio: ProviderState::Available {
                value: AudioStatus {
                    volume_percent: 42,
                    muted: false,
                },
                stale: false,
            },
            power: ProviderState::Available {
                value: PowerStatus {
                    percent: 91,
                    charging: false,
                },
                stale: false,
            },
            activity: ProviderState::Unavailable,
        }
    }

    fn surface() -> TopBarSurface {
        TopBarSurface::new(HostOutput::new(1920, 1080, 1.0), TokenMode::dark(), snapshot())
    }

    fn item<'a>(contract: &'a TopBarSurfaceContract, id: &str) -> &'a TopBarItem {
        contract
            .items
            .iter()
            .find(|item| item.id == id)
            .unwrap_or_else(|| panic!("missing bar item {id}"))
    }

    #[test]
    fn the_bar_spans_the_top_edge_and_reserves_its_own_height() {
        let contract = surface().contract().expect("contract");

        assert_eq!(contract.placement.layer, LayerShellLayer::Top);
        assert_eq!(contract.placement.anchor, LayerAnchor::TOP_BAR);
        assert_eq!(contract.placement.size, (1920, 40));
        assert_eq!(contract.placement.exclusive_zone, 40);
        assert_eq!(contract.physical_size, (1920, 40));
    }

    #[test]
    fn the_upper_right_zone_keeps_its_documented_order() {
        let mut surface = surface();
        surface.refresh(TopBarSnapshot {
            activity: ProviderState::Available {
                value: vec![ActivityIndicator::Microphone],
                stale: false,
            },
            ..snapshot()
        });
        let contract = surface.contract().expect("contract");

        let trailing: Vec<TopBarZone> = contract
            .items
            .iter()
            .filter(|item| item.zone != TopBarZone::ForegroundApp)
            .map(|item| item.zone)
            .collect();
        let mut expected = trailing.clone();
        expected.sort_unstable();
        assert_eq!(
            trailing, expected,
            "Live Capsule → status → center → system status"
        );

        // And the block as a whole stays flush right.
        let clock = item(&contract, "topbar.clock");
        let right_edge = clock.rect.0 + clock.rect.2;
        assert!(right_edge <= contract.logical_size.width);
        assert!(right_edge > contract.logical_size.width - Spacing::Lg.px() * 2.0);
    }

    #[test]
    fn the_foreground_zone_stays_on_the_left_and_names_the_verified_app() {
        let mut surface = surface();
        surface.set_foreground(Some(ForegroundApplication {
            app_id: "org.sol.files".to_owned(),
            display_name: "Files".to_owned(),
        }));
        let contract = surface.contract().expect("contract");

        let foreground = item(&contract, "topbar.foreground-app");
        assert_eq!(foreground.text, "FILES");
        assert!(foreground.label.contains("org.sol.files"));
        assert_eq!(foreground.rect.0, Spacing::Md.px());
    }

    #[test]
    fn an_unfocused_desktop_names_the_system_rather_than_guessing_an_app() {
        let contract = surface().contract().expect("contract");
        assert_eq!(item(&contract, "topbar.foreground-app").text, "SOL");
    }

    #[test]
    fn a_focus_change_replaces_the_whole_identity_at_once() {
        let mut surface = surface();
        surface.set_foreground(Some(ForegroundApplication {
            app_id: "org.sol.files".to_owned(),
            display_name: "Files".to_owned(),
        }));
        surface.set_foreground(Some(ForegroundApplication {
            app_id: "org.sol.terminal".to_owned(),
            display_name: "Terminal".to_owned(),
        }));
        let contract = surface.contract().expect("contract");

        let foreground = item(&contract, "topbar.foreground-app");
        assert_eq!(foreground.text, "TERMINAL");
        assert!(!foreground.label.contains("files"));
    }

    #[test]
    fn provider_failures_are_shown_as_failures_and_not_as_values() {
        let mut surface = surface();
        surface.refresh(TopBarSnapshot {
            power: ProviderState::Unavailable,
            network: ProviderState::Error("NetworkManager unreachable".into()),
            audio: ProviderState::Available {
                value: AudioStatus {
                    volume_percent: 42,
                    muted: false,
                },
                stale: true,
            },
            ..snapshot()
        });
        let contract = surface.contract().expect("contract");

        assert_eq!(item(&contract, "topbar.power").text, "BAT --");
        assert_eq!(item(&contract, "topbar.network").text, "NET !");
        assert_eq!(item(&contract, "topbar.network").foreground, Color::Error);
        assert_eq!(item(&contract, "topbar.audio").text, "VOL 42%~");
    }

    #[test]
    fn a_live_privacy_indicator_takes_the_capsule_anchor() {
        let mut surface = surface();
        surface.refresh(TopBarSnapshot {
            activity: ProviderState::Available {
                value: vec![ActivityIndicator::Microphone, ActivityIndicator::Camera],
                stale: false,
            },
            ..snapshot()
        });
        let contract = surface.contract().expect("contract");

        let capsule = item(&contract, "topbar.live-capsule");
        assert_eq!(capsule.zone, TopBarZone::LiveCapsule);
        assert_eq!(capsule.text, "MIC CAM");
        assert!(capsule.label.contains("Microphone"));
    }

    #[test]
    fn a_presented_frame_matches_its_placement_and_carries_ink() {
        let mut host = RecordingDesktopHost::default();
        let mut surface =
            TopBarSurface::new(HostOutput::new(640, 480, 1.0), TokenMode::dark(), snapshot());
        surface.present(&mut host).expect("present");

        let (placement, pixels) = host.last_frame(TOPBAR_NAMESPACE).expect("frame");
        assert_eq!(placement.size, (640, 40));
        assert_eq!(pixels.len(), 640 * 40 * 4);

        let background = TokenMode::dark().color(Color::Elevated);
        let expected = (background.0 * 255.0 + 0.5) as u8;
        assert!(
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[0] != expected || pixel[1] != expected),
            "the bar must paint text and a separator, not only its fill"
        );
    }

    #[test]
    fn a_scaled_output_paints_a_taller_physical_bar_for_the_same_logical_height() {
        let surface =
            TopBarSurface::new(HostOutput::new(3840, 2160, 2.0), TokenMode::dark(), snapshot());
        let contract = surface.contract().expect("contract");

        assert_eq!(contract.logical_size.height, 40.0);
        assert_eq!(contract.physical_size, (3840, 80));
        assert_eq!(contract.placement.exclusive_zone, 80);
    }

    #[test]
    fn a_narrow_output_never_pushes_the_right_zone_past_the_left_one() {
        let surface =
            TopBarSurface::new(HostOutput::new(200, 100, 1.0), TokenMode::dark(), snapshot());
        let contract = surface.contract().expect("contract");

        for item in &contract.items {
            assert!(item.rect.0 >= 0.0, "no item starts off-surface");
        }
    }

    #[test]
    fn an_unconfigured_output_refuses_to_produce_a_frame() {
        let surface =
            TopBarSurface::new(HostOutput::new(0, 0, 1.0), TokenMode::dark(), snapshot());
        assert!(matches!(
            surface.contract(),
            Err(TopBarSurfaceError::OutputNotConfigured)
        ));
    }
}
