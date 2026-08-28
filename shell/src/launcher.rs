//! Renderer-neutral Dock, Launcher, and local Search model.
//!
//! The model owns ordered pins, catalog lookup, deterministic local ranking,
//! and keyboard/accessibility semantics. SCP activation and window close
//! are deliberately outside it: a concrete desktop adapter must opt in rather
//! than a headless fixture pretending that a desktop action occurred.

use std::{collections::BTreeMap, error::Error, fmt};

use sol_app::{AppId, AppIdentity};
use sol_design::color::Color;
use sol_graphics::Surface;
use sol_system::{
    ActionSource, SystemAction, SystemActionApi, SystemActionRequest, SystemActionResult,
};
use sol_ui::{AccessibilityNode, Button, InteractionTree, Key, KeyboardOutcome, SemanticControl};

/// The fixed caller identity for requests originating in SOL's trusted shell.
const SHELL_APP_ID: &str = "org.sol.shell";

/// One launcher-visible application. Catalogs are supplied by a package/app
/// discovery adapter; the model has no filesystem crawler or network client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppCatalogEntry {
    /// Stable cross-service identity and user-visible title.
    pub identity: AppIdentity,
    /// Local, package-provided search aliases such as `editor` or `terminal`.
    pub keywords: Vec<String>,
}

impl AppCatalogEntry {
    /// Create an application catalog entry.
    #[must_use]
    pub fn new(identity: AppIdentity, keywords: impl IntoIterator<Item = String>) -> Self {
        Self {
            identity,
            keywords: keywords.into_iter().collect(),
        }
    }

    /// Return the durable application ID.
    #[must_use]
    pub fn app_id(&self) -> &AppId {
        self.identity.app_id()
    }
}

/// Privacy boundary selected for the first search index implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchPrivacy {
    /// Search only caller-provided application catalog metadata in memory.
    /// No file, document, clipboard, telemetry, or network source is queried.
    #[default]
    LocalCatalogOnly,
}

/// How a result matched a local catalog term. This is intentionally explainable
/// rather than heuristic/ML based, so ranking is repeatable in tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMatch {
    /// Exact or prefix title match.
    Title,
    /// Prefix or substring of the reverse-DNS app ID.
    AppId,
    /// Match of a package-provided local keyword.
    Keyword,
}

/// One ranked search result. Search results carry their typed launch intent;
/// no arbitrary program, argv, URL, or shell string crosses this boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    /// Matching application identity.
    pub entry: AppCatalogEntry,
    /// Explainable matching category.
    pub matched_on: SearchMatch,
    /// Higher ranks first; ties resolve by stable AppId spelling.
    pub score: u16,
}

impl SearchResult {
    /// Return the permission-aware intent to launch this known application.
    #[must_use]
    pub fn action(&self) -> SystemAction {
        SystemAction::LaunchApplication {
            app_id: self.entry.app_id().clone(),
        }
    }
}

/// Stable, local-only application search index. Indexing is explicit and
/// replaces an existing entry for the same AppId, so package refreshes cannot
/// create ambiguous duplicate launch targets.
#[derive(Debug, Default, Clone)]
pub struct LocalSearchIndex {
    entries: BTreeMap<AppId, AppCatalogEntry>,
}

impl LocalSearchIndex {
    /// Add or replace a local application record.
    pub fn upsert(&mut self, entry: AppCatalogEntry) {
        self.entries.insert(entry.app_id().clone(), entry);
    }

    /// Return the deliberate Phase 4 privacy policy.
    #[must_use]
    pub const fn privacy(&self) -> SearchPrivacy {
        SearchPrivacy::LocalCatalogOnly
    }

    /// Query applications with deterministic, local ranking.
    #[must_use]
    pub fn query(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        let query = normalize(query);
        if query.is_empty() || limit == 0 {
            return Vec::new();
        }
        let mut results: Vec<_> = self
            .entries
            .values()
            .filter_map(|entry| score(entry, &query))
            .collect();
        results.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.entry.app_id().cmp(right.entry.app_id()))
        });
        results.truncate(limit);
        results
    }

    /// Return a local catalog entry by stable identity.
    #[must_use]
    pub fn get(&self, app_id: &AppId) -> Option<&AppCatalogEntry> {
        self.entries.get(app_id)
    }

    /// Iterate catalog entries in stable AppId order.
    pub fn entries(&self) -> impl Iterator<Item = &AppCatalogEntry> {
        self.entries.values()
    }
}

fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn score(entry: &AppCatalogEntry, query: &str) -> Option<SearchResult> {
    let title = normalize(entry.identity.display_name());
    let app_id = entry.app_id().as_str();
    let (matched_on, score) = if title == query {
        (SearchMatch::Title, 1_000)
    } else if title.starts_with(query) {
        (SearchMatch::Title, 800)
    } else if entry
        .keywords
        .iter()
        .any(|keyword| normalize(keyword).starts_with(query))
    {
        (SearchMatch::Keyword, 600)
    } else if app_id.starts_with(query) {
        (SearchMatch::AppId, 400)
    } else if title.contains(query) {
        (SearchMatch::Title, 300)
    } else if entry
        .keywords
        .iter()
        .any(|keyword| normalize(keyword).contains(query))
    {
        (SearchMatch::Keyword, 200)
    } else if app_id.contains(query) {
        (SearchMatch::AppId, 100)
    } else {
        return None;
    };
    Some(SearchResult {
        entry: entry.clone(),
        matched_on,
        score,
    })
}

/// One explicit desktop operation, identified by `AppId` instead of a window
/// handle or shell command. The compositor/session adapter decides whether it
/// can act on the current desktop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesktopAction {
    /// Start a cataloged application after authorization.
    Launch(AppId),
    /// Bring an observed running application to the foreground.
    Activate(AppId),
    /// Request closing an observed running application.
    Close(AppId),
}

/// A native session bridge for desktop actions. A recording fixture is useful
/// in CI, but it is not evidence that an SCP client was activated or closed.
pub trait DesktopActionAdapter {
    /// Attempt one typed desktop action.
    fn perform(&mut self, action: DesktopAction) -> Result<(), DesktopActionError>;
}

/// Result from a desktop-action adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesktopActionError {
    /// No concrete session integration is installed.
    Unavailable,
    /// A platform adapter refused the typed request.
    Rejected(String),
}

impl fmt::Display for DesktopActionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => formatter.write_str("desktop action adapter is unavailable"),
            Self::Rejected(reason) => write!(formatter, "desktop action rejected: {reason}"),
        }
    }
}

impl Error for DesktopActionError {}

/// Safe production-default adapter until shell↔compositor activation and close
/// transport is implemented.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnavailableDesktopAdapter;

impl DesktopActionAdapter for UnavailableDesktopAdapter {
    fn perform(&mut self, _action: DesktopAction) -> Result<(), DesktopActionError> {
        Err(DesktopActionError::Unavailable)
    }
}

/// Deterministic fixture that records requests without touching a desktop.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RecordingDesktopAdapter {
    /// Actions submitted in order.
    pub actions: Vec<DesktopAction>,
}

impl DesktopActionAdapter for RecordingDesktopAdapter {
    fn perform(&mut self, action: DesktopAction) -> Result<(), DesktopActionError> {
        self.actions.push(action);
        Ok(())
    }
}

/// Launcher/Dock model error.
#[derive(Debug)]
pub enum ShellModelError {
    /// The selected app is not in the local launcher catalog.
    UnknownApplication(AppId),
    /// The selected app has no observed running instance for this session.
    NotRunning(AppId),
    /// Permission service did not accept the typed request.
    Authorization(String),
    /// The installed session adapter could not perform the typed action.
    Desktop(DesktopActionError),
}

impl fmt::Display for ShellModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownApplication(app_id) => write!(formatter, "unknown application {app_id}"),
            Self::NotRunning(app_id) => write!(formatter, "application {app_id} is not running"),
            Self::Authorization(reason) => {
                write!(formatter, "action authorization failed: {reason}")
            }
            Self::Desktop(error) => error.fmt(formatter),
        }
    }
}

impl Error for ShellModelError {}

/// Authorization outcome displayed by launcher/search UI. Only `Performed`
/// means an installed desktop adapter accepted a request; `AwaitingConsent` and
/// `Denied` never reach it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionOutcome {
    /// An authorized action was submitted to the desktop adapter.
    Performed,
    /// A trusted consent surface must resolve this request first.
    AwaitingConsent,
    /// Permission policy denied the request.
    Denied,
}

/// Renderer-neutral dock item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockItem {
    /// Launcher catalog metadata.
    pub entry: AppCatalogEntry,
    /// Whether the user pinned this item.
    pub pinned: bool,
    /// Whether a trusted session observer reports it running.
    pub running: bool,
}

/// Minimal model projection for a renderer. The shell's native SCP top bar may
/// consume this state later, without leaking protocol objects into tests.
#[derive(Debug, Clone, PartialEq)]
pub struct ShellLauncherFrame {
    /// Surface size after its fractional scale is applied.
    pub pixel_size: (u32, u32),
    /// Dock entries in presentation order.
    pub dock: Vec<DockItem>,
    /// SOL token roles, not concrete theme values.
    pub background: Color,
    /// SOL token role for text/icon foreground.
    pub foreground: Color,
}

/// Dock + Launcher controller. `A` authorizes typed launch intent; `D` is the
/// only component permitted to attempt a real desktop action.
pub struct ShellLauncher<A: SystemActionApi, D: DesktopActionAdapter> {
    actions: A,
    desktop: D,
    index: LocalSearchIndex,
    pinned: Vec<AppId>,
    running: BTreeMap<AppId, bool>,
    tree: InteractionTree,
}

impl<A: SystemActionApi, D: DesktopActionAdapter> ShellLauncher<A, D> {
    /// Create the shell model from an explicit local application catalog.
    #[must_use]
    pub fn new(actions: A, desktop: D, entries: impl IntoIterator<Item = AppCatalogEntry>) -> Self {
        let mut index = LocalSearchIndex::default();
        for entry in entries {
            index.upsert(entry);
        }
        let mut tree = InteractionTree::new("shell-launcher", "SOL Launcher");
        tree.push(SemanticControl::button(
            "open-launcher",
            &Button::new().with_label("Open launcher"),
        ));
        for entry in index.entries() {
            tree.push(SemanticControl::Button {
                id: sol_ui::SemanticId::new(format!("launch:{}", entry.app_id())),
                label: entry.identity.display_name().to_owned(),
                enabled: true,
            });
        }
        Self {
            actions,
            desktop,
            index,
            pinned: Vec::new(),
            running: BTreeMap::new(),
            tree,
        }
    }

    /// Pin an application without changing process state.
    pub fn pin(&mut self, app_id: &AppId) -> Result<(), ShellModelError> {
        self.require_known(app_id)?;
        if !self.pinned.contains(app_id) {
            self.pinned.push(app_id.clone());
        }
        Ok(())
    }

    /// Remove a user pin. Running items remain visible in the dock.
    pub fn unpin(&mut self, app_id: &AppId) {
        self.pinned.retain(|item| item != app_id);
    }

    /// Update session-observed running state. This does not claim an app was
    /// launched by this shell and is intentionally supplied by an integration.
    pub fn observe_running(&mut self, app_id: AppId, running: bool) {
        self.running.insert(app_id, running);
    }

    /// Return pins followed by unpinned running applications in stable AppId order.
    #[must_use]
    pub fn dock_items(&self) -> Vec<DockItem> {
        let mut ids = self.pinned.clone();
        for (app_id, running) in &self.running {
            if *running && !ids.contains(app_id) {
                ids.push(app_id.clone());
            }
        }
        ids.into_iter()
            .filter_map(|app_id| {
                self.index.get(&app_id).cloned().map(|entry| DockItem {
                    pinned: self.pinned.contains(&app_id),
                    running: self.running.get(&app_id).copied().unwrap_or(false),
                    entry,
                })
            })
            .collect()
    }

    /// Query the local-only application catalog.
    #[must_use]
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        self.index.query(query, limit)
    }

    /// Execute an application search result through the permission boundary.
    pub fn execute_search(
        &mut self,
        result: &SearchResult,
    ) -> Result<ActionOutcome, ShellModelError> {
        self.launch(result.entry.app_id())
    }

    /// Request a launch for a cataloged application. The actual adapter call is
    /// possible only after `SystemActionApi` returns an authorization.
    pub fn launch(&mut self, app_id: &AppId) -> Result<ActionOutcome, ShellModelError> {
        self.require_known(app_id)?;
        let result = self.request_launch(app_id)?;
        match result {
            SystemActionResult::Authorized(_authorization) => {
                self.desktop
                    .perform(DesktopAction::Launch(app_id.clone()))
                    .map_err(ShellModelError::Desktop)?;
                Ok(ActionOutcome::Performed)
            }
            SystemActionResult::AwaitingUserConsent(_) => Ok(ActionOutcome::AwaitingConsent),
            SystemActionResult::Denied { .. } => Ok(ActionOutcome::Denied),
        }
    }

    /// Request session activation for an observed running app. No fallback
    /// process launch is attempted, and the default adapter returns unavailable.
    pub fn activate(&mut self, app_id: &AppId) -> Result<(), ShellModelError> {
        self.require_running(app_id)?;
        self.desktop
            .perform(DesktopAction::Activate(app_id.clone()))
            .map_err(ShellModelError::Desktop)
    }

    /// Request closing an observed running app through the session adapter.
    pub fn close(&mut self, app_id: &AppId) -> Result<(), ShellModelError> {
        self.require_running(app_id)?;
        self.desktop
            .perform(DesktopAction::Close(app_id.clone()))
            .map_err(ShellModelError::Desktop)
    }

    /// Give launcher buttons normal SolUI keyboard navigation.
    pub fn handle_key(&mut self, key: Key) -> KeyboardOutcome {
        self.tree.handle_key(key)
    }
    /// Expose renderer-independent accessibility metadata.
    #[must_use]
    pub fn accessibility_tree(&self) -> AccessibilityNode {
        self.tree.accessibility_tree()
    }
    /// Resolve dock data for an abstract SOL graphics surface.
    #[must_use]
    pub fn frame_for(&self, surface: &Surface) -> ShellLauncherFrame {
        ShellLauncherFrame {
            pixel_size: (
                (surface.size.0 * surface.scale) as u32,
                (surface.size.1 * surface.scale) as u32,
            ),
            dock: self.dock_items(),
            background: Color::Elevated,
            foreground: Color::TextPrimary,
        }
    }
    /// Borrow the test/native adapter after operations have been recorded.
    #[must_use]
    pub fn desktop(&self) -> &D {
        &self.desktop
    }

    fn request_launch(&self, app_id: &AppId) -> Result<SystemActionResult, ShellModelError> {
        let caller = AppId::parse(SHELL_APP_ID)
            .map_err(|error| ShellModelError::Authorization(error.to_string()))?;
        self.actions
            .request(SystemActionRequest {
                caller,
                source: ActionSource::ShellLauncher,
                action: SystemAction::LaunchApplication {
                    app_id: app_id.clone(),
                },
            })
            .map_err(action_error)
    }
    fn require_known(&self, app_id: &AppId) -> Result<(), ShellModelError> {
        self.index
            .get(app_id)
            .map(|_| ())
            .ok_or_else(|| ShellModelError::UnknownApplication(app_id.clone()))
    }
    fn require_running(&self, app_id: &AppId) -> Result<(), ShellModelError> {
        if self.running.get(app_id).copied().unwrap_or(false) {
            Ok(())
        } else {
            Err(ShellModelError::NotRunning(app_id.clone()))
        }
    }
}

fn action_error(error: impl fmt::Display) -> ShellModelError {
    ShellModelError::Authorization(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sol_system::{
        ConsentDecision, DefaultDenyPolicy, MemoryActionAuditStore, MemoryPermissionStore,
        PermissionPolicy, PolicyDecision, SystemActionService, SystemCapability,
    };

    #[derive(Debug, Clone, Copy)]
    struct PromptLaunch;
    impl PermissionPolicy for PromptLaunch {
        fn decide(
            &self,
            _request: &SystemActionRequest,
            _capability: SystemCapability,
        ) -> PolicyDecision {
            PolicyDecision::RequireUserConsent {
                rationale: "launch selected application".into(),
            }
        }
    }
    fn catalog() -> Vec<AppCatalogEntry> {
        vec![
            AppCatalogEntry::new(
                AppIdentity::new(AppId::parse("org.sol.terminal").unwrap(), "Terminal").unwrap(),
                vec!["shell".into(), "console".into()],
            ),
            AppCatalogEntry::new(
                AppIdentity::new(AppId::parse("org.sol.settings").unwrap(), "Settings").unwrap(),
                vec!["preferences".into()],
            ),
        ]
    }
    fn terminal_id() -> AppId {
        AppId::parse("org.sol.terminal").unwrap()
    }

    #[test]
    fn local_search_is_private_explainable_and_stably_ranked() {
        let mut index = LocalSearchIndex::default();
        for entry in catalog() {
            index.upsert(entry);
        }
        assert_eq!(index.privacy(), SearchPrivacy::LocalCatalogOnly);
        let result = index.query("term", 5);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].matched_on, SearchMatch::Title);
        assert_eq!(result[0].score, 800);
        assert!(matches!(
            result[0].action(),
            SystemAction::LaunchApplication { .. }
        ));
        assert!(index.query("", 5).is_empty());
    }

    #[test]
    fn denied_search_never_reaches_desktop_adapter() {
        let service = SystemActionService::new(
            DefaultDenyPolicy,
            MemoryPermissionStore::default(),
            MemoryActionAuditStore::default(),
        );
        let mut shell = ShellLauncher::new(service, RecordingDesktopAdapter::default(), catalog());
        let result = shell.search("terminal", 1).pop().unwrap();
        assert_eq!(
            shell.execute_search(&result).unwrap(),
            ActionOutcome::Denied
        );
        assert!(shell.desktop().actions.is_empty());
    }

    #[test]
    fn consent_then_authorized_launch_is_typed_and_recorded() {
        let service = SystemActionService::new(
            PromptLaunch,
            MemoryPermissionStore::default(),
            MemoryActionAuditStore::default(),
        );
        let terminal = terminal_id();
        let pending = service
            .request(SystemActionRequest {
                caller: AppId::parse(SHELL_APP_ID).unwrap(),
                source: ActionSource::ShellLauncher,
                action: SystemAction::LaunchApplication {
                    app_id: terminal.clone(),
                },
            })
            .unwrap();
        let SystemActionResult::AwaitingUserConsent(consent) = pending else {
            panic!("launch must stop at consent boundary");
        };
        service
            .resolve_user_consent(consent.consent_id, ConsentDecision::AllowAlways)
            .unwrap();
        let mut shell = ShellLauncher::new(service, RecordingDesktopAdapter::default(), catalog());
        assert_eq!(shell.launch(&terminal).unwrap(), ActionOutcome::Performed);
        assert_eq!(
            shell.desktop().actions,
            vec![DesktopAction::Launch(terminal)]
        );
    }

    #[test]
    fn dock_pins_running_state_navigation_and_unavailable_activation_are_explicit() {
        let service = SystemActionService::new(
            DefaultDenyPolicy,
            MemoryPermissionStore::default(),
            MemoryActionAuditStore::default(),
        );
        let terminal = terminal_id();
        let mut shell = ShellLauncher::new(service, UnavailableDesktopAdapter, catalog());
        shell.pin(&terminal).unwrap();
        shell.observe_running(terminal.clone(), true);
        assert_eq!(
            shell.dock_items(),
            vec![DockItem {
                entry: catalog().remove(0),
                pinned: true,
                running: true
            }]
        );
        assert!(matches!(
            shell.handle_key(Key::Tab),
            KeyboardOutcome::FocusMoved(_)
        ));
        assert!(shell.accessibility_tree().children[0].state.focused);
        assert!(matches!(
            shell.activate(&terminal),
            Err(ShellModelError::Desktop(DesktopActionError::Unavailable))
        ));
        let frame = shell.frame_for(&Surface::high_dpi(400.0, 64.0, 1.25));
        assert_eq!(frame.pixel_size, (500, 80));
    }

    #[test]
    fn unknown_or_not_running_actions_have_no_desktop_side_effect() {
        let service = SystemActionService::new(
            DefaultDenyPolicy,
            MemoryPermissionStore::default(),
            MemoryActionAuditStore::default(),
        );
        let mut shell = ShellLauncher::new(service, RecordingDesktopAdapter::default(), catalog());
        let terminal = terminal_id();
        assert!(matches!(
            shell.close(&terminal),
            Err(ShellModelError::NotRunning(_))
        ));
        let unknown = AppId::parse("org.sol.unknown").unwrap();
        assert!(matches!(
            shell.launch(&unknown),
            Err(ShellModelError::UnknownApplication(_))
        ));
        assert!(shell.desktop().actions.is_empty());
    }
}
