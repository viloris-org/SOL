//! The SOL desktop session.
//!
//! This is the object that *is* the desktop: it owns the four Shell surfaces —
//! background, top bar, Dock, Launcher — decides when each repaints, and routes
//! compositor input to whichever of them the user aimed at.
//!
//! ```text
//! compositor ──SCP──▶ ScpDesktopHost ──▶ DesktopSession ──▶ surfaces
//!      ▲                                       │
//!      └───────────── committed frames ────────┘
//! ```
//!
//! ## Repainting is event-driven
//!
//! Shell surfaces repaint when their content changes, not once per vblank. A
//! wallpaper that has not changed does not need to allocate and commit an
//! output-sized buffer sixty times a second, and the compositor composes from
//! the last committed buffer regardless.
//!
//! ## Every launch still goes through the permission boundary
//!
//! The Dock and the Launcher both resolve a user gesture to an `AppId` and then
//! hand it to [`ShellLauncher`], which is the only component that may ask the
//! typed `SystemActionApi` for authorization. Neither surface can start a
//! process, so there is exactly one path a launch can take and exactly one
//! place to audit it.

use std::sync::atomic::{AtomicBool, Ordering};

use sol_app::AppId;
use sol_compositor::scp::protocol::{
    ButtonState, CompositorMessage, InputEvent, KeyState, SurfaceId,
};
use sol_design::accessibility::TokenMode;
use sol_ui::Key;

use crate::{
    desktop_surface::{DESKTOP_NAMESPACE, DesktopSurface, DesktopSurfaceError},
    dock_surface::{DOCK_NAMESPACE, DockSurface, DockSurfaceError, DockTarget},
    launcher::{
        ActionOutcome, AppCatalogEntry, DesktopActionAdapter, ShellLauncher, ShellModelError,
    },
    launcher_surface::{
        LAUNCHER_NAMESPACE, LauncherOutcome, LauncherSurface, LauncherSurfaceError,
    },
    scp_host::{DesktopHost, DesktopHostError, HostOutput, ScpDesktopHost},
    topbar::TopBarSnapshot,
    topbar_surface::{ForegroundApplication, TOPBAR_NAMESPACE, TopBarSurface, TopBarSurfaceError},
};

use sol_system::SystemActionApi;

/// Left pointer button, in the evdev numbering SCP forwards.
const BUTTON_LEFT: u32 = 0x110;

/// What the session did with an event, so a caller can stop the loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionFlow {
    /// Keep running.
    Continue,
    /// The compositor ended the session.
    Stop,
}

/// A failure inside the desktop session.
#[derive(Debug)]
pub enum DesktopSessionError {
    Host(DesktopHostError),
    Desktop(DesktopSurfaceError),
    TopBar(TopBarSurfaceError),
    Dock(DockSurfaceError),
    Launcher(LauncherSurfaceError),
    /// A launch was refused by the typed authorization boundary.
    Model(ShellModelError),
}

impl std::fmt::Display for DesktopSessionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Host(error) => error.fmt(formatter),
            Self::Desktop(error) => error.fmt(formatter),
            Self::TopBar(error) => error.fmt(formatter),
            Self::Dock(error) => error.fmt(formatter),
            Self::Launcher(error) => error.fmt(formatter),
            Self::Model(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for DesktopSessionError {}

macro_rules! session_error_from {
    ($source:ty, $variant:ident) => {
        impl From<$source> for DesktopSessionError {
            fn from(error: $source) -> Self {
                Self::$variant(error)
            }
        }
    };
}

session_error_from!(DesktopHostError, Host);
session_error_from!(DesktopSurfaceError, Desktop);
session_error_from!(TopBarSurfaceError, TopBar);
session_error_from!(DockSurfaceError, Dock);
session_error_from!(LauncherSurfaceError, Launcher);
session_error_from!(ShellModelError, Model);

/// The desktop: four Shell surfaces and the policy that drives them.
pub struct DesktopSession<H: DesktopHost, A: SystemActionApi, D: DesktopActionAdapter> {
    host: H,
    output: HostOutput,
    mode: TokenMode,
    desktop: DesktopSurface,
    topbar: TopBarSurface,
    dock: DockSurface,
    launcher: LauncherSurface,
    model: ShellLauncher<A, D>,
    /// Surface-local pointer position, tracked per surface because a button
    /// press carries no coordinates of its own.
    pointer: Option<(SurfaceId, f64, f64)>,
}

impl<H: DesktopHost, A: SystemActionApi, D: DesktopActionAdapter> DesktopSession<H, A, D> {
    /// Assemble a desktop over a host, a catalog, and a launch authority.
    pub fn new(
        host: H,
        output: HostOutput,
        mode: TokenMode,
        model: ShellLauncher<A, D>,
        catalog: impl IntoIterator<Item = AppCatalogEntry>,
        status: TopBarSnapshot,
    ) -> Self {
        let catalog: Vec<AppCatalogEntry> = catalog.into_iter().collect();
        Self {
            host,
            output,
            mode,
            desktop: DesktopSurface::new(output, mode),
            topbar: TopBarSurface::new(output, mode, status),
            dock: DockSurface::new(output, mode, Vec::new()),
            launcher: LauncherSurface::new(output, mode, catalog),
            model,
            pointer: None,
        }
    }

    /// Borrow the native host, for event pumping and lifecycle.
    pub fn host_mut(&mut self) -> &mut H {
        &mut self.host
    }

    /// Borrow the launcher model, for pins and observed running state.
    pub fn model_mut(&mut self) -> &mut ShellLauncher<A, D> {
        &mut self.model
    }

    /// The output the desktop is currently laid out for.
    #[must_use]
    pub const fn output(&self) -> HostOutput {
        self.output
    }

    /// Whether the Launcher is presented.
    #[must_use]
    pub const fn is_launcher_open(&self) -> bool {
        self.launcher.is_visible()
    }

    /// Adopt a new output extent and repaint everything against it.
    ///
    /// All four surfaces move together: a mode change that resized only some of
    /// them would leave the Dock centered on an output width no longer in
    /// effect.
    pub fn set_output(&mut self, output: HostOutput) -> Result<(), DesktopSessionError> {
        if output == self.output || !output.is_configured() {
            return Ok(());
        }
        self.output = output;
        self.desktop.set_output(output);
        self.topbar.set_output(output);
        self.dock.set_output(output);
        self.launcher.set_output(output);
        self.present_all()
    }

    /// Adopt new theme and accessibility preferences across the desktop.
    pub fn set_token_mode(&mut self, mode: TokenMode) -> Result<(), DesktopSessionError> {
        self.mode = mode;
        self.desktop.set_token_mode(mode);
        self.topbar.set_token_mode(mode);
        self.dock.set_token_mode(mode);
        self.launcher.set_token_mode(mode);
        self.present_all()
    }

    /// Present every surface that should currently be mapped.
    ///
    /// Ordered bottom-up so the first frame the compositor composes is already
    /// a complete desktop rather than a bar floating over nothing.
    pub fn present_all(&mut self) -> Result<(), DesktopSessionError> {
        self.desktop.present(&mut self.host)?;
        self.dock.refresh(self.model.dock_items());
        self.dock.present(&mut self.host)?;
        self.topbar.present(&mut self.host)?;
        self.launcher.present(&mut self.host)?;
        Ok(())
    }

    /// Replace the top bar's provider snapshot and repaint just the bar.
    pub fn refresh_status(&mut self, status: TopBarSnapshot) -> Result<(), DesktopSessionError> {
        self.topbar.refresh(status);
        self.topbar.present(&mut self.host)?;
        Ok(())
    }

    /// Record the compositor-authenticated foreground application.
    pub fn set_foreground(
        &mut self,
        foreground: Option<ForegroundApplication>,
    ) -> Result<(), DesktopSessionError> {
        let focused = foreground
            .as_ref()
            .and_then(|application| AppId::parse(&application.app_id).ok());
        self.topbar.set_foreground(foreground);
        self.dock.set_focused(focused);
        self.dock.refresh(self.model.dock_items());
        self.topbar.present(&mut self.host)?;
        self.dock.present(&mut self.host)?;
        Ok(())
    }

    /// Repaint the Dock after the launcher model's pins or running set changed.
    pub fn refresh_dock(&mut self) -> Result<(), DesktopSessionError> {
        self.dock.refresh(self.model.dock_items());
        self.dock.present(&mut self.host)?;
        Ok(())
    }

    /// Open or close the Launcher, as `Super+A` and the Dock entry do.
    pub fn toggle_launcher(&mut self) -> Result<(), DesktopSessionError> {
        self.launcher.toggle(&mut self.host)?;
        Ok(())
    }

    /// Route one key to the surface that owns the keyboard.
    ///
    /// Only the Launcher takes keyboard focus, so a key with no Launcher up is
    /// not the desktop's to consume — it belongs to the focused application,
    /// and the compositor never delivered it here in the first place.
    pub fn handle_key(&mut self, key: Key) -> Result<Option<ActionOutcome>, DesktopSessionError> {
        if !self.launcher.is_visible() {
            return Ok(None);
        }
        match self.launcher.handle_key(key) {
            LauncherOutcome::Ignored => Ok(None),
            LauncherOutcome::Changed => {
                self.launcher.present(&mut self.host)?;
                Ok(None)
            }
            LauncherOutcome::Dismissed => {
                self.launcher.close(&mut self.host)?;
                Ok(None)
            }
            LauncherOutcome::Launch(app_id) => self.launch(&app_id).map(Some),
        }
    }

    /// Act on a Dock target the user aimed at.
    pub fn activate(
        &mut self,
        target: DockTarget,
    ) -> Result<Option<ActionOutcome>, DesktopSessionError> {
        match target {
            DockTarget::Launcher => {
                self.toggle_launcher()?;
                Ok(None)
            }
            DockTarget::Application(app_id) => self.launch(&app_id).map(Some),
        }
    }

    /// Ask the launcher model to launch an application.
    ///
    /// The Launcher closes only when something actually started. A denied or
    /// pending request leaves it open, because closing it would tell the user
    /// their click worked when it did not.
    pub fn launch(&mut self, app_id: &AppId) -> Result<ActionOutcome, DesktopSessionError> {
        let outcome = self.model.launch(app_id)?;
        if outcome == ActionOutcome::Performed {
            if self.launcher.is_visible() {
                self.launcher.close(&mut self.host)?;
            }
            self.refresh_dock()?;
        }
        Ok(outcome)
    }
}

impl<A: SystemActionApi, D: DesktopActionAdapter> DesktopSession<ScpDesktopHost, A, D> {
    /// Bring the desktop up against a live compositor.
    ///
    /// The background is presented first and alone: its configure is what
    /// reports the output extent, and every other surface is laid out against
    /// that rather than against a size the Shell assumed.
    pub fn start(&mut self) -> Result<(), DesktopSessionError> {
        self.desktop.present(&mut self.host)?;
        let output = self.host.output();
        if output.is_configured() && output != self.output {
            self.output = output;
            self.desktop.set_output(output);
            self.topbar.set_output(output);
            self.dock.set_output(output);
            self.launcher.set_output(output);
            self.desktop.present(&mut self.host)?;
        }
        // The background is already current. Present only the chrome here so
        // startup does not allocate and transfer another output-sized frame.
        self.dock.refresh(self.model.dock_items());
        self.dock.present(&mut self.host)?;
        self.topbar.present(&mut self.host)?;
        self.launcher.present(&mut self.host)?;
        Ok(())
    }

    /// Drain and act on everything the compositor has sent.
    pub fn pump(&mut self) -> Result<SessionFlow, DesktopSessionError> {
        for event in self.host.poll()? {
            if self.handle_event(event)? == SessionFlow::Stop {
                return Ok(SessionFlow::Stop);
            }
        }
        Ok(SessionFlow::Continue)
    }

    /// Act on one compositor event.
    pub fn handle_event(
        &mut self,
        event: CompositorMessage,
    ) -> Result<SessionFlow, DesktopSessionError> {
        match event {
            CompositorMessage::OutputChanged {
                width,
                height,
                scale,
            } => {
                self.set_output(HostOutput::new(width, height, scale as f32))?;
            }
            CompositorMessage::ConfigureLayerSurface {
                layer_id,
                serial,
                width,
                height,
            } => {
                // A configure after creation is the compositor changing its
                // mind about our geometry. Acknowledging it and repainting is
                // the whole contract; ignoring it leaves the surface committed
                // at a size the compositor is no longer laying out for.
                self.host.ack_layer_configure(layer_id, serial)?;
                self.set_output(HostOutput::new(width, height, self.output.scale))?;
            }
            CompositorMessage::LayerSurfaceClosed { layer_id } => {
                // The compositor withdrew one of our surfaces. Forget it, then
                // repaint: a Shell that stops presenting because one surface
                // went away is a Shell that leaves the user with no Dock.
                if let Some(namespace) = self.host.forget_closed(layer_id) {
                    tracing::warn!(%namespace, "compositor closed a Shell surface; recreating it");
                    self.recreate(&namespace)?;
                }
            }
            CompositorMessage::InputEvent { surface_id, event } => {
                return self.handle_input(surface_id, event);
            }
            CompositorMessage::ProtocolError {
                code,
                message,
                fatal,
            } => {
                tracing::warn!(%code, %message, fatal, "SCP protocol error");
                if fatal {
                    return Ok(SessionFlow::Stop);
                }
            }
            // Frame callbacks and buffer releases confirm the compositor is
            // done with a frame. The desktop repaints on change rather than on
            // cadence, so neither prompts work of its own.
            CompositorMessage::FrameCallback { .. } | CompositorMessage::BufferRelease { .. } => {}
            _ => {}
        }
        Ok(SessionFlow::Continue)
    }

    /// Run until `running` is cleared or the compositor ends the session.
    pub fn run(&mut self, running: &AtomicBool) -> Result<(), DesktopSessionError> {
        while running.load(Ordering::Acquire) {
            if self.pump()? == SessionFlow::Stop {
                return Ok(());
            }
        }
        Ok(())
    }

    fn recreate(&mut self, namespace: &str) -> Result<(), DesktopSessionError> {
        match namespace {
            DESKTOP_NAMESPACE => self.desktop.present(&mut self.host)?,
            TOPBAR_NAMESPACE => self.topbar.present(&mut self.host)?,
            DOCK_NAMESPACE => self.dock.present(&mut self.host)?,
            // A Launcher the compositor closed is a Launcher the user no longer
            // has open; recreating it would put an overlay back on screen that
            // nothing asked for.
            LAUNCHER_NAMESPACE => {}
            other => tracing::warn!(%other, "compositor closed an unknown Shell surface"),
        }
        Ok(())
    }

    fn handle_input(
        &mut self,
        surface_id: SurfaceId,
        event: InputEvent,
    ) -> Result<SessionFlow, DesktopSessionError> {
        match event {
            InputEvent::PointerEnter { x, y, .. } | InputEvent::PointerMotion { x, y, .. } => {
                self.pointer = Some((surface_id, x, y));
            }
            InputEvent::PointerLeave { .. } => {
                if self
                    .pointer
                    .is_some_and(|(owner, _, _)| owner == surface_id)
                {
                    self.pointer = None;
                }
            }
            InputEvent::PointerButton {
                button,
                state: ButtonState::Pressed,
                ..
            } if button == BUTTON_LEFT => {
                self.handle_click(surface_id)?;
            }
            InputEvent::KeyboardKey {
                key,
                state: KeyState::Pressed,
                ..
            } => {
                if let Some(key) = keycode_to_key(key) {
                    self.handle_key(key)?;
                }
            }
            _ => {}
        }
        Ok(SessionFlow::Continue)
    }

    fn handle_click(&mut self, surface_id: SurfaceId) -> Result<(), DesktopSessionError> {
        let Some((owner, x, y)) = self.pointer else {
            return Ok(());
        };
        if owner != surface_id {
            return Ok(());
        }
        let Some(namespace) = self.host.namespace_of(surface_id).map(str::to_owned) else {
            return Ok(());
        };

        // The compositor reports surface-local coordinates in physical pixels;
        // every surface lays itself out in logical ones.
        let (x, y) = (x as f32 / self.output.scale, y as f32 / self.output.scale);

        match namespace.as_str() {
            DOCK_NAMESPACE => {
                if let Some(target) = self.dock.hit_test(x, y) {
                    self.activate(target)?;
                }
            }
            LAUNCHER_NAMESPACE => {
                if let Some(app_id) = self.launcher.hit_test(x, y) {
                    self.launch(&app_id)?;
                }
            }
            // A click on the wallpaper dismisses transient system UI, which is
            // what makes the Launcher escapable without the keyboard.
            DESKTOP_NAMESPACE if self.launcher.is_visible() => {
                self.launcher.close(&mut self.host)?;
            }
            _ => {}
        }
        Ok(())
    }
}

/// Map an SCP keycode to a SolUI key.
///
/// SCP speaks the XKB keycode space (evdev + 8). The Shell needs only the keys
/// its own surfaces consume, so this is a fixed table rather than a keymap
/// interpreter: layout-correct text entry belongs to the input-method path, and
/// pretending otherwise here would produce a search field that types the wrong
/// characters on a non-US layout.
fn keycode_to_key(keycode: u32) -> Option<Key> {
    const LETTER_ROWS: [(u32, &str); 3] = [(24, "qwertyuiop"), (38, "asdfghjkl"), (52, "zxcvbnm")];

    match keycode {
        9 => return Some(Key::Escape),
        22 => return Some(Key::Backspace),
        23 => return Some(Key::Tab),
        36 => return Some(Key::Enter),
        65 => return Some(Key::Space),
        113 => return Some(Key::ArrowLeft),
        114 => return Some(Key::ArrowRight),
        // The digit row: `1`..`9` then `0`.
        10..=19 => {
            let digit = if keycode == 19 { 0 } else { keycode - 9 };
            return char::from_digit(digit, 10).map(Key::Character);
        }
        _ => {}
    }

    for (base, letters) in LETTER_ROWS {
        if keycode >= base && keycode < base + letters.len() as u32 {
            return letters
                .chars()
                .nth((keycode - base) as usize)
                .map(Key::Character);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        launcher::RecordingDesktopAdapter,
        scp_host::RecordingDesktopHost,
        topbar::{ClockStatus, ProviderState},
    };
    use sol_app::AppIdentity;
    use sol_system::{
        DefaultDenyPolicy, MemoryActionAuditStore, MemoryPermissionStore, SystemActionService,
    };

    type TestSession = DesktopSession<
        RecordingDesktopHost,
        SystemActionService<DefaultDenyPolicy, MemoryPermissionStore, MemoryActionAuditStore>,
        RecordingDesktopAdapter,
    >;

    fn entry(app_id: &str, name: &str) -> AppCatalogEntry {
        AppCatalogEntry::new(
            AppIdentity::new(AppId::parse(app_id).expect("valid app id"), name)
                .expect("valid app identity"),
            Vec::new(),
        )
    }

    fn catalog() -> Vec<AppCatalogEntry> {
        vec![
            entry("org.sol.files", "Files"),
            entry("org.sol.terminal", "Terminal"),
        ]
    }

    fn status() -> TopBarSnapshot {
        TopBarSnapshot {
            clock: ProviderState::Available {
                value: ClockStatus {
                    time: "09:41".into(),
                    date: "2026-08-28".into(),
                },
                stale: false,
            },
            workspace: ProviderState::Unavailable,
            network: ProviderState::Unavailable,
            audio: ProviderState::Unavailable,
            power: ProviderState::Unavailable,
            activity: ProviderState::Unavailable,
        }
    }

    fn session() -> TestSession {
        let model = ShellLauncher::new(
            SystemActionService::new(
                DefaultDenyPolicy,
                MemoryPermissionStore::default(),
                MemoryActionAuditStore::default(),
            ),
            RecordingDesktopAdapter::default(),
            catalog(),
        );
        DesktopSession::new(
            RecordingDesktopHost::default(),
            HostOutput::new(1920, 1080, 1.0),
            TokenMode::dark(),
            model,
            catalog(),
            status(),
        )
    }

    #[test]
    fn a_first_paint_maps_the_background_the_dock_and_the_bar_but_not_the_launcher() {
        let mut session = session();
        session.present_all().expect("present");

        let namespaces: Vec<&str> = session
            .host
            .presented
            .iter()
            .map(|(placement, _)| placement.namespace.as_str())
            .collect();
        assert_eq!(
            namespaces,
            [DESKTOP_NAMESPACE, DOCK_NAMESPACE, TOPBAR_NAMESPACE]
        );
    }

    #[test]
    fn the_background_is_presented_before_the_chrome_above_it() {
        let mut session = session();
        session.present_all().expect("present");

        assert_eq!(
            session.host.presented[0].0.namespace, DESKTOP_NAMESPACE,
            "the desktop composes bottom-up"
        );
    }

    #[test]
    fn toggling_opens_and_then_withdraws_the_launcher() {
        let mut session = session();
        session.toggle_launcher().expect("open");
        assert!(session.is_launcher_open());
        assert!(session.host.last_frame(LAUNCHER_NAMESPACE).is_some());

        session.toggle_launcher().expect("close");
        assert!(!session.is_launcher_open());
        assert_eq!(session.host.dismissed, vec![LAUNCHER_NAMESPACE.to_owned()]);
    }

    #[test]
    fn keys_are_ignored_while_the_launcher_is_closed() {
        let mut session = session();
        assert_eq!(session.handle_key(Key::Character('f')).expect("key"), None);
        assert!(session.host.presented.is_empty());
    }

    #[test]
    fn escape_closes_the_launcher_the_same_way_the_toggle_does() {
        let mut session = session();
        session.toggle_launcher().expect("open");
        session.handle_key(Key::Escape).expect("escape");

        assert!(!session.is_launcher_open());
        assert_eq!(session.host.dismissed, vec![LAUNCHER_NAMESPACE.to_owned()]);
    }

    #[test]
    fn a_launch_goes_through_authorization_and_default_deny_starts_nothing() {
        let mut session = session();
        session.toggle_launcher().expect("open");
        let outcome = session.handle_key(Key::Enter).expect("activate");

        assert_eq!(outcome, Some(ActionOutcome::Denied));
        assert!(
            session.is_launcher_open(),
            "a denied launch must not look like a successful one"
        );
        assert!(session.model.desktop().actions.is_empty());
    }

    #[test]
    fn the_dock_launcher_entry_toggles_rather_than_launching() {
        let mut session = session();
        assert_eq!(
            session.activate(DockTarget::Launcher).expect("activate"),
            None
        );
        assert!(session.is_launcher_open());
    }

    #[test]
    fn observed_running_applications_reach_the_dock() {
        let mut session = session();
        session.present_all().expect("present");
        session
            .model_mut()
            .observe_running(AppId::parse("org.sol.terminal").expect("valid"), true);
        session.refresh_dock().expect("refresh");

        let contract = session.dock.last_contract.clone().expect("contract");
        assert_eq!(
            contract.tiles.len(),
            2,
            "launcher entry plus one running app"
        );
        assert_eq!(contract.tiles[1].label, "Terminal");
        assert!(contract.tiles[1].running);
    }

    #[test]
    fn a_focus_change_updates_both_the_bar_and_the_dock() {
        let mut session = session();
        session.present_all().expect("present");
        session
            .model_mut()
            .observe_running(AppId::parse("org.sol.files").expect("valid"), true);
        session
            .set_foreground(Some(ForegroundApplication {
                app_id: "org.sol.files".to_owned(),
                display_name: "Files".to_owned(),
            }))
            .expect("focus");

        let bar = session.topbar.last_contract.clone().expect("bar");
        assert_eq!(bar.items[0].text, "FILES");
        let dock = session.dock.last_contract.clone().expect("dock");
        assert!(dock.accessibility.children[1].state.focused);
    }

    #[test]
    fn a_new_output_extent_relays_out_every_surface_at_once() {
        let mut session = session();
        session.toggle_launcher().expect("open");
        session
            .set_output(HostOutput::new(1280, 720, 1.0))
            .expect("resize");

        for namespace in [
            DESKTOP_NAMESPACE,
            DOCK_NAMESPACE,
            TOPBAR_NAMESPACE,
            LAUNCHER_NAMESPACE,
        ] {
            let (placement, pixels) = session
                .host
                .last_frame(namespace)
                .unwrap_or_else(|| panic!("{namespace} was not repainted"));
            assert_eq!(
                pixels.len(),
                (placement.size.0 * placement.size.1 * 4) as usize
            );
        }
        assert_eq!(
            session
                .host
                .last_frame(DESKTOP_NAMESPACE)
                .expect("desktop")
                .0
                .size,
            (1280, 720)
        );
    }

    #[test]
    fn an_unchanged_output_extent_does_not_repaint_the_desktop() {
        let mut session = session();
        session.present_all().expect("present");
        let painted = session.host.presented.len();
        session
            .set_output(HostOutput::new(1920, 1080, 1.0))
            .expect("same extent");

        assert_eq!(session.host.presented.len(), painted);
    }

    #[test]
    fn a_status_refresh_repaints_only_the_bar() {
        let mut session = session();
        session.present_all().expect("present");
        let painted = session.host.presented.len();
        session
            .refresh_status(TopBarSnapshot {
                clock: ProviderState::Unavailable,
                ..status()
            })
            .expect("refresh");

        assert_eq!(session.host.presented.len(), painted + 1);
        assert_eq!(
            session.host.presented[painted].0.namespace,
            TOPBAR_NAMESPACE
        );
    }

    #[test]
    fn scp_keycodes_map_onto_the_keys_the_shell_consumes() {
        assert_eq!(keycode_to_key(9), Some(Key::Escape));
        assert_eq!(keycode_to_key(36), Some(Key::Enter));
        assert_eq!(keycode_to_key(24), Some(Key::Character('q')));
        assert_eq!(keycode_to_key(38), Some(Key::Character('a')));
        assert_eq!(keycode_to_key(52), Some(Key::Character('z')));
        assert_eq!(keycode_to_key(10), Some(Key::Character('1')));
        assert_eq!(keycode_to_key(19), Some(Key::Character('0')));
        // A key the Shell has no meaning for is not invented into one.
        assert_eq!(keycode_to_key(133), None);
        assert_eq!(keycode_to_key(0), None);
    }
}
