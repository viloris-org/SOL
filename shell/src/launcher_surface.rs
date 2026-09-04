//! The native Application Launcher surface.
//!
//! The Launcher is the Shell's application library: a Dock-anchored overlay
//! holding the authenticated `.app` catalog, with unified search over it.
//!
//! ## It selects; it does not launch
//!
//! Activating a tile produces a [`LauncherOutcome::Launch`], not a process.
//! Every launch still goes through [`crate::launcher::ShellLauncher`] and its
//! typed `SystemActionApi` authorization, exactly as a search result or a Dock
//! click does. A surface that could start an application directly would be a
//! second, weaker path around the permission boundary — and the one an attacker
//! would aim at, because it is the one the user is most likely to click.
//!
//! ## Ranking is the catalog's, not the Launcher's
//!
//! Results come from [`crate::launcher::LocalSearchIndex`], whose ranking is
//! explainable and local-only. The surface never reorders them, which is what
//! makes "an application cannot purchase visual priority" a property of the
//! system rather than a promise about this module's good behavior.

use sol_app::AppId;
use sol_design::{
    accessibility::TokenMode,
    color::Color,
    metrics::ControlMetric,
    motion::{Motion, MotionSpec},
    radius::Radius,
    spacing::Spacing,
    typography::FontStyle,
};
use sol_ui::{AccessibilityNode, AccessibilityState, Key, LogicalSize, SemanticId, SemanticRole};

use crate::{
    launcher::{AppCatalogEntry, LocalSearchIndex, SearchMatch},
    overlay::{LayerShellLayer, LogicalPoint},
    paint::{Canvas, PixelRect, text_height, text_scale_for_height},
    scp_host::{
        DesktopHost, DesktopHostError, HostOutput, LayerAnchor, LayerKeyboard, LayerMargin,
        LayerPlacement,
    },
};

/// Stable SCP namespace of the Launcher.
pub const LAUNCHER_NAMESPACE: &str = "sol.launcher";

/// Tiles per row.
///
/// A fixed grid rather than a width-derived one: the Launcher must present the
/// same application in the same place across outputs and scales, because the
/// user's muscle memory is for a position, not for a reflow rule.
const COLUMNS: usize = 4;

/// Rows presented at once. Anything past them is reported as overflow rather
/// than silently dropped.
const MAX_ROWS: usize = 4;

/// Placeholder shown in an empty search field.
const SEARCH_PLACEHOLDER: &str = "SEARCH APPS";

/// What the Launcher asks the Shell to do next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LauncherOutcome {
    /// The key did not apply.
    Ignored,
    /// The visible frame changed and must be repainted.
    Changed,
    /// The user asked to close the Launcher.
    Dismissed,
    /// The user activated an application. The Shell must route this through the
    /// launcher model's typed authorization before anything starts.
    Launch(AppId),
}

/// One laid-out application tile.
#[derive(Debug, Clone, PartialEq)]
pub struct LauncherTile {
    /// Stable identity, used for accessibility and hit testing.
    pub id: String,
    /// Application this tile launches.
    pub app_id: AppId,
    /// Verified display name.
    pub label: String,
    /// Single-character mark painted in the tile.
    pub mark: String,
    /// Why this entry matched, when a query is active.
    pub matched_on: Option<SearchMatch>,
    /// Whether the keyboard selection is on this tile.
    pub selected: bool,
    /// Logical rectangle `(x, y, width, height)` within the Launcher surface.
    pub rect: (f32, f32, f32, f32),
}

/// A complete Launcher frame.
///
/// Not `PartialEq`: `MotionSpec` is not comparable, and comparing whole frames
/// would compare a motion curve as if it were layout. Tests compare the parts
/// that carry meaning — placement, tiles, query — instead.
#[derive(Debug, Clone)]
pub struct LauncherSurfaceContract {
    pub output: HostOutput,
    pub logical_size: LogicalSize,
    pub physical_size: (u32, u32),
    pub placement: LayerPlacement,
    /// Where the Launcher's presentation originates, in output-logical
    /// coordinates. Motion grows from the Dock's Launcher entry and collapses
    /// back along the same path.
    pub origin_anchor: LogicalPoint,
    pub transition: MotionSpec,
    /// Token-resolved surface roles.
    pub background: Color,
    pub border: Color,
    pub accent: Color,
    pub foreground: Color,
    pub secondary: Color,
    pub radius: Radius,
    pub typography: FontStyle,
    pub token_mode: TokenMode,
    /// The active query, empty when the full library is shown.
    pub query: String,
    /// Logical rectangle of the search field.
    pub search_rect: (f32, f32, f32, f32),
    /// Visible tiles in ranked order.
    pub tiles: Vec<LauncherTile>,
    /// Matching entries the grid could not show.
    pub overflow: usize,
    pub accessibility: AccessibilityNode,
}

/// Errors raised before an invalid Launcher frame reaches the compositor.
#[derive(Debug)]
pub enum LauncherSurfaceError {
    /// The compositor has not reported a usable output extent yet.
    OutputNotConfigured,
    /// The output is too small to place the Launcher on.
    OutputTooSmall(LogicalSize),
    /// The frame extent could not be allocated.
    UnpaintableExtent((u32, u32)),
    /// The native host rejected the frame.
    Host(DesktopHostError),
}

impl std::fmt::Display for LauncherSurfaceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutputNotConfigured => {
                formatter.write_str("no output extent has been configured yet")
            }
            Self::OutputTooSmall(size) => write!(
                formatter,
                "a {}x{} output cannot hold the Launcher",
                size.width, size.height
            ),
            Self::UnpaintableExtent((width, height)) => {
                write!(formatter, "cannot paint a {width}x{height} Launcher")
            }
            Self::Host(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for LauncherSurfaceError {}

impl From<DesktopHostError> for LauncherSurfaceError {
    fn from(error: DesktopHostError) -> Self {
        Self::Host(error)
    }
}

/// Retained Launcher surface.
#[derive(Debug, Clone)]
pub struct LauncherSurface {
    output: HostOutput,
    mode: TokenMode,
    index: LocalSearchIndex,
    query: String,
    selected: usize,
    visible: bool,
    /// Last frame presented, retained for inspection and native hosts.
    pub last_contract: Option<LauncherSurfaceContract>,
}

impl LauncherSurface {
    /// Create the Launcher over an application catalog.
    #[must_use]
    pub fn new(
        output: HostOutput,
        mode: TokenMode,
        entries: impl IntoIterator<Item = AppCatalogEntry>,
    ) -> Self {
        let mut index = LocalSearchIndex::default();
        for entry in entries {
            index.upsert(entry);
        }
        Self {
            output,
            mode,
            index,
            query: String::new(),
            selected: 0,
            visible: false,
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

    /// Whether the Launcher is currently presented.
    #[must_use]
    pub const fn is_visible(&self) -> bool {
        self.visible
    }

    /// The active query.
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Open the Launcher with a cleared query and the first entry selected.
    ///
    /// Opening always resets: a Launcher that reopens holding the last search
    /// shows the user a filtered library they did not ask for and cannot see
    /// the cause of.
    pub fn open(&mut self, host: &mut impl DesktopHost) -> Result<(), LauncherSurfaceError> {
        self.query.clear();
        self.selected = 0;
        self.visible = true;
        self.present(host)
    }

    /// Close the Launcher and release its surface.
    pub fn close(&mut self, host: &mut impl DesktopHost) -> Result<(), LauncherSurfaceError> {
        self.visible = false;
        self.last_contract = None;
        host.dismiss(LAUNCHER_NAMESPACE)?;
        Ok(())
    }

    /// Toggle the Launcher, as `Super+A` and the Dock entry do.
    pub fn toggle(&mut self, host: &mut impl DesktopHost) -> Result<(), LauncherSurfaceError> {
        if self.visible {
            self.close(host)
        } else {
            self.open(host)
        }
    }

    /// Route one key.
    ///
    /// Returns what the Shell should do; it never performs it. Repainting is
    /// the caller's decision too, so a session can coalesce several keys into
    /// one frame.
    pub fn handle_key(&mut self, key: Key) -> LauncherOutcome {
        if !self.visible {
            return LauncherOutcome::Ignored;
        }
        let visible = self.visible_entries().len();
        match key {
            Key::Escape => {
                self.visible = false;
                LauncherOutcome::Dismissed
            }
            Key::Character(character) => {
                self.query.push(character);
                self.selected = 0;
                LauncherOutcome::Changed
            }
            Key::Backspace => {
                if self.query.pop().is_none() {
                    return LauncherOutcome::Ignored;
                }
                self.selected = 0;
                LauncherOutcome::Changed
            }
            Key::Tab | Key::ArrowRight => self.move_selection(1, visible),
            Key::ShiftTab | Key::ArrowLeft => self.move_selection(-1, visible),
            Key::Enter | Key::Space => self
                .visible_entries()
                .get(self.selected)
                .map_or(LauncherOutcome::Ignored, |(entry, _)| {
                    LauncherOutcome::Launch(entry.app_id().clone())
                }),
            Key::CommandPalette => LauncherOutcome::Ignored,
        }
    }

    /// Resolve a pointer position, in Launcher-surface logical coordinates.
    #[must_use]
    pub fn hit_test(&self, x: f32, y: f32) -> Option<AppId> {
        let contract = self.last_contract.as_ref()?;
        contract
            .tiles
            .iter()
            .find(|tile| {
                x >= tile.rect.0
                    && x < tile.rect.0 + tile.rect.2
                    && y >= tile.rect.1
                    && y < tile.rect.1 + tile.rect.3
            })
            .map(|tile| tile.app_id.clone())
    }

    /// Build the current frame contract without painting it.
    pub fn contract(&self) -> Result<LauncherSurfaceContract, LauncherSurfaceError> {
        if !self.output.is_configured() {
            return Err(LauncherSurfaceError::OutputNotConfigured);
        }

        let padding = Spacing::Lg.px();
        let gap = Spacing::Md.px();
        let cell = ControlMetric::Toolbar.spec().height;
        let tile_width = ControlMetric::Button.spec().min_width;
        let label_scale = text_scale_for_height(self.mode.typography(FontStyle::Label).pixels);
        let tile_height = cell + Spacing::Xs.px() + text_height(label_scale);
        let search_height = ControlMetric::TextField.spec().height;

        let entries = self.visible_entries();
        let shown = entries.len().min(COLUMNS * MAX_ROWS);
        let rows = shown.div_ceil(COLUMNS).max(1);

        let width = padding * 2.0 + COLUMNS as f32 * tile_width + (COLUMNS - 1) as f32 * gap;
        let height = padding * 2.0
            + search_height
            + gap
            + rows as f32 * tile_height
            + (rows - 1) as f32 * gap;

        let logical_output = self.output.logical_size();
        if width > logical_output.width || height > logical_output.height {
            return Err(LauncherSurfaceError::OutputTooSmall(logical_output));
        }

        let grid_top = padding + search_height + gap;
        let tiles: Vec<LauncherTile> = entries
            .iter()
            .take(shown)
            .enumerate()
            .map(|(index, (entry, matched_on))| {
                let column = index % COLUMNS;
                let row = index / COLUMNS;
                let name = entry.identity.display_name().to_owned();
                LauncherTile {
                    id: format!("launcher.{}", entry.app_id()),
                    app_id: entry.app_id().clone(),
                    mark: initial(&name),
                    label: name,
                    matched_on: *matched_on,
                    selected: index == self.selected,
                    rect: (
                        padding + column as f32 * (tile_width + gap),
                        grid_top + row as f32 * (tile_height + gap),
                        tile_width,
                        tile_height,
                    ),
                }
            })
            .collect();

        let physical = (
            self.output.physical(width).max(1),
            self.output.physical(height).max(1),
        );

        Ok(LauncherSurfaceContract {
            output: self.output,
            logical_size: LogicalSize::new(width, height),
            physical_size: (physical.0 as u32, physical.1 as u32),
            placement: LayerPlacement {
                namespace: LAUNCHER_NAMESPACE.to_owned(),
                // Overlay: the Launcher is transient system UI and must sit
                // above panels, including the Dock it grew out of.
                layer: LayerShellLayer::Overlay,
                // No edge anchored on either axis: the compositor centers it.
                anchor: LayerAnchor::default(),
                margin: LayerMargin::default(),
                size: physical,
                exclusive_zone: 0,
                // The Launcher is the one Shell surface that owns the keyboard
                // while it is up; typing goes to its search field, not to the
                // window behind it.
                keyboard: LayerKeyboard::Exclusive,
            },
            // The Dock is bottom-centered, so its Launcher entry is the origin
            // the presentation grows from.
            origin_anchor: LogicalPoint {
                x: logical_output.width / 2.0,
                y: logical_output.height,
            },
            transition: self.mode.motion_spec(Motion::Panel),
            background: Color::Elevated,
            border: Color::Border,
            accent: Color::Accent,
            foreground: Color::TextPrimary,
            secondary: Color::TextSecondary,
            radius: Radius::Md,
            typography: FontStyle::Label,
            token_mode: self.mode,
            query: self.query.clone(),
            search_rect: (padding, padding, width - padding * 2.0, search_height),
            overflow: entries.len().saturating_sub(shown),
            accessibility: accessibility_tree(&tiles, &self.query, entries.len()),
            tiles,
        })
    }

    /// Paint and present the Launcher, if it is open.
    pub fn present(&mut self, host: &mut impl DesktopHost) -> Result<(), LauncherSurfaceError> {
        if !self.visible {
            return Ok(());
        }
        let contract = self.contract()?;
        let pixels = rasterize(&contract)?;
        host.present(&contract.placement, &pixels)?;
        self.last_contract = Some(contract);
        Ok(())
    }

    /// Entries to present: ranked matches for a query, or the whole catalog.
    fn visible_entries(&self) -> Vec<(AppCatalogEntry, Option<SearchMatch>)> {
        if self.query.trim().is_empty() {
            return self
                .index
                .entries()
                .cloned()
                .map(|entry| (entry, None))
                .collect();
        }
        self.index
            .query(&self.query, COLUMNS * MAX_ROWS)
            .into_iter()
            .map(|result| (result.entry, Some(result.matched_on)))
            .collect()
    }

    fn move_selection(&mut self, delta: isize, visible: usize) -> LauncherOutcome {
        if visible == 0 {
            return LauncherOutcome::Ignored;
        }
        let shown = visible.min(COLUMNS * MAX_ROWS) as isize;
        let next = (self.selected as isize + delta).rem_euclid(shown);
        self.selected = next as usize;
        LauncherOutcome::Changed
    }
}

fn initial(name: &str) -> String {
    name.chars()
        .next()
        .map_or_else(|| "?".to_owned(), |first| first.to_uppercase().to_string())
}

fn accessibility_tree(tiles: &[LauncherTile], query: &str, matches: usize) -> AccessibilityNode {
    let mut children = vec![AccessibilityNode {
        id: SemanticId::new("launcher.search"),
        role: SemanticRole::TextField,
        label: "Search applications".to_owned(),
        value: Some(query.to_owned()),
        state: AccessibilityState {
            focused: true,
            selected: false,
            disabled: false,
            editable: true,
        },
        children: Vec::new(),
    }];
    children.extend(tiles.iter().map(|tile| AccessibilityNode {
        id: SemanticId::new(tile.id.clone()),
        role: SemanticRole::Button,
        label: tile.label.clone(),
        value: Some(tile.app_id.to_string()),
        state: AccessibilityState {
            focused: tile.selected,
            selected: tile.selected,
            disabled: false,
            editable: false,
        },
        children: Vec::new(),
    }));

    AccessibilityNode {
        id: SemanticId::new("launcher-surface"),
        role: SemanticRole::Group,
        label: "Application Launcher".to_owned(),
        value: Some(format!("{matches} applications")),
        state: AccessibilityState::default(),
        children,
    }
}

/// Paint one Launcher frame.
fn rasterize(contract: &LauncherSurfaceContract) -> Result<Vec<u8>, LauncherSurfaceError> {
    let (width, height) = contract.physical_size;
    let mut canvas = Canvas::new(width, height)
        .ok_or(LauncherSurfaceError::UnpaintableExtent((width, height)))?;
    let mode = contract.token_mode;
    let scale = contract.output.scale;
    let panel = PixelRect::new(0.0, 0.0, width as f32, height as f32);
    canvas.fill_rounded_rect(
        panel,
        contract.radius.px() * scale,
        mode.color(contract.background),
    );

    let label_scale = text_scale_for_height(mode.typography(contract.typography).pixels) * scale;
    let search = PixelRect::new(
        contract.search_rect.0 * scale,
        contract.search_rect.1 * scale,
        contract.search_rect.2 * scale,
        contract.search_rect.3 * scale,
    );
    canvas.fill_rounded_rect(
        search,
        Radius::Sm.px() * scale,
        mode.color(Color::HoverOverlay),
    );
    let (query_text, query_color) = if contract.query.is_empty() {
        (SEARCH_PLACEHOLDER.to_owned(), contract.secondary)
    } else {
        (contract.query.to_uppercase(), contract.foreground)
    };
    canvas.draw_text(
        (
            search.x + Spacing::Sm.px() * scale,
            search.center_y() - text_height(label_scale) / 2.0,
        ),
        label_scale,
        mode.color(query_color),
        &query_text,
    );

    for tile in &contract.tiles {
        let rect = PixelRect::new(
            tile.rect.0 * scale,
            tile.rect.1 * scale,
            tile.rect.2 * scale,
            tile.rect.3 * scale,
        );
        if tile.selected {
            canvas.fill_rounded_rect(rect, Radius::Sm.px() * scale, mode.color(contract.accent));
        }

        let cell = ControlMetric::Toolbar.spec().height * scale;
        let mark_width = Canvas::text_width(label_scale, &tile.mark);
        canvas.draw_text(
            (
                rect.center_x() - mark_width / 2.0,
                rect.y + cell / 2.0 - text_height(label_scale) / 2.0,
            ),
            label_scale,
            mode.color(if tile.selected {
                Color::TextOnAccent
            } else {
                contract.foreground
            }),
            &tile.mark,
        );

        let label = tile.label.to_uppercase();
        let label_width = Canvas::text_width(label_scale, &label);
        canvas.draw_text(
            (
                rect.center_x() - label_width / 2.0,
                rect.y + cell + Spacing::Xs.px() * scale,
            ),
            label_scale,
            mode.color(if tile.selected {
                Color::TextOnAccent
            } else {
                contract.secondary
            }),
            &label,
        );
    }

    Ok(canvas.into_pixels())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scp_host::RecordingDesktopHost;
    use sol_app::AppIdentity;

    fn entry(app_id: &str, name: &str, keywords: &[&str]) -> AppCatalogEntry {
        AppCatalogEntry::new(
            AppIdentity::new(AppId::parse(app_id).expect("valid app id"), name)
                .expect("valid app identity"),
            keywords.iter().map(|keyword| (*keyword).to_owned()),
        )
    }

    fn catalog() -> Vec<AppCatalogEntry> {
        vec![
            entry("org.sol.files", "Files", &["browser"]),
            entry("org.sol.settings", "Settings", &["preferences"]),
            entry("org.sol.terminal", "Terminal", &["console", "shell"]),
        ]
    }

    fn surface() -> LauncherSurface {
        LauncherSurface::new(
            HostOutput::new(1920, 1080, 1.0),
            TokenMode::dark(),
            catalog(),
        )
    }

    fn opened() -> (LauncherSurface, RecordingDesktopHost) {
        let mut host = RecordingDesktopHost::default();
        let mut surface = surface();
        surface.open(&mut host).expect("open");
        (surface, host)
    }

    #[test]
    fn the_launcher_is_closed_until_it_is_opened() {
        let mut host = RecordingDesktopHost::default();
        let mut surface = surface();
        assert!(!surface.is_visible());

        surface.present(&mut host).expect("present while closed");
        assert!(host.presented.is_empty());
    }

    #[test]
    fn opening_presents_a_centered_overlay_that_owns_the_keyboard() {
        let (surface, host) = opened();
        let contract = surface.last_contract.clone().expect("contract");

        assert_eq!(contract.placement.layer, LayerShellLayer::Overlay);
        assert_eq!(contract.placement.anchor, LayerAnchor::default());
        assert_eq!(contract.placement.keyboard, LayerKeyboard::Exclusive);
        assert_eq!(contract.placement.exclusive_zone, 0);
        assert!(host.last_frame(LAUNCHER_NAMESPACE).is_some());
    }

    #[test]
    fn the_presentation_originates_at_the_docks_launcher_anchor() {
        let (surface, _) = opened();
        let contract = surface.last_contract.expect("contract");

        assert_eq!(contract.origin_anchor.x, 960.0);
        assert_eq!(contract.origin_anchor.y, 1080.0);
    }

    #[test]
    fn an_empty_query_shows_the_whole_catalog_in_stable_order() {
        let (surface, _) = opened();
        let contract = surface.last_contract.expect("contract");

        let labels: Vec<&str> = contract
            .tiles
            .iter()
            .map(|tile| tile.label.as_str())
            .collect();
        assert_eq!(labels, ["Files", "Settings", "Terminal"]);
        assert_eq!(contract.overflow, 0);
    }

    #[test]
    fn typing_filters_by_the_catalogs_own_ranking() {
        let (mut surface, _) = opened();
        assert_eq!(
            surface.handle_key(Key::Character('t')),
            LauncherOutcome::Changed
        );
        let contract = surface.contract().expect("contract");

        assert_eq!(contract.query, "t");
        // Both entries match, and the catalog's ranking decides the order: a
        // title prefix outranks a title substring. The surface does not reorder.
        let labels: Vec<&str> = contract
            .tiles
            .iter()
            .map(|tile| tile.label.as_str())
            .collect();
        assert_eq!(labels, ["Terminal", "Settings"]);
        assert_eq!(contract.tiles[0].matched_on, Some(SearchMatch::Title));
    }

    #[test]
    fn a_keyword_match_is_reported_as_a_keyword_match() {
        let (mut surface, _) = opened();
        for character in "console".chars() {
            surface.handle_key(Key::Character(character));
        }
        let contract = surface.contract().expect("contract");

        assert_eq!(contract.tiles.len(), 1);
        assert_eq!(contract.tiles[0].matched_on, Some(SearchMatch::Keyword));
    }

    #[test]
    fn backspace_widens_the_query_again_and_stops_at_empty() {
        let (mut surface, _) = opened();
        surface.handle_key(Key::Character('t'));
        assert_eq!(surface.handle_key(Key::Backspace), LauncherOutcome::Changed);
        assert_eq!(surface.query(), "");
        assert_eq!(surface.handle_key(Key::Backspace), LauncherOutcome::Ignored);
    }

    #[test]
    fn selection_wraps_within_the_visible_results() {
        let (mut surface, _) = opened();
        surface.handle_key(Key::ArrowLeft);
        let contract = surface.contract().expect("contract");
        assert!(contract.tiles[2].selected, "selection wraps to the end");

        surface.handle_key(Key::ArrowRight);
        let contract = surface.contract().expect("contract");
        assert!(contract.tiles[0].selected);
    }

    #[test]
    fn a_narrowed_query_resets_the_selection_to_the_top_result() {
        let (mut surface, _) = opened();
        surface.handle_key(Key::ArrowRight);
        surface.handle_key(Key::Character('s'));
        let contract = surface.contract().expect("contract");

        assert!(contract.tiles[0].selected);
    }

    #[test]
    fn activation_asks_the_shell_to_launch_rather_than_launching() {
        let (mut surface, _) = opened();
        let outcome = surface.handle_key(Key::Enter);

        assert_eq!(
            outcome,
            LauncherOutcome::Launch(AppId::parse("org.sol.files").expect("valid"))
        );
        assert!(
            surface.is_visible(),
            "the Launcher does not close itself on a request the Shell may still deny"
        );
    }

    #[test]
    fn activating_with_no_match_does_nothing() {
        let (mut surface, _) = opened();
        for character in "zzz".chars() {
            surface.handle_key(Key::Character(character));
        }
        assert_eq!(surface.handle_key(Key::Enter), LauncherOutcome::Ignored);
    }

    #[test]
    fn escape_dismisses_and_a_closed_launcher_ignores_further_keys() {
        let (mut surface, _) = opened();
        assert_eq!(surface.handle_key(Key::Escape), LauncherOutcome::Dismissed);
        assert!(!surface.is_visible());
        assert_eq!(
            surface.handle_key(Key::Character('a')),
            LauncherOutcome::Ignored
        );
    }

    #[test]
    fn closing_withdraws_the_surface_and_clears_its_hit_targets() {
        let (mut surface, mut host) = opened();
        surface.close(&mut host).expect("close");

        assert_eq!(host.dismissed, vec![LAUNCHER_NAMESPACE.to_owned()]);
        assert_eq!(surface.hit_test(60.0, 80.0), None);
    }

    #[test]
    fn reopening_starts_from_an_empty_query() {
        let (mut surface, mut host) = opened();
        surface.handle_key(Key::Character('t'));
        surface.close(&mut host).expect("close");
        surface.open(&mut host).expect("reopen");

        assert_eq!(surface.query(), "");
        assert_eq!(surface.contract().expect("contract").tiles.len(), 3);
    }

    #[test]
    fn a_pointer_inside_a_tile_resolves_to_that_application() {
        let (surface, _) = opened();
        let contract = surface.last_contract.clone().expect("contract");
        let settings = &contract.tiles[1];

        assert_eq!(
            surface.hit_test(settings.rect.0 + 1.0, settings.rect.1 + 1.0),
            Some(AppId::parse("org.sol.settings").expect("valid"))
        );
        assert_eq!(surface.hit_test(0.0, 0.0), None);
    }

    #[test]
    fn a_catalog_larger_than_the_grid_reports_its_overflow() {
        let mut entries = catalog();
        for index in 0..20 {
            entries.push(entry(
                &format!("org.sol.app{index}"),
                &format!("App{index}"),
                &[],
            ));
        }
        let mut host = RecordingDesktopHost::default();
        let mut surface =
            LauncherSurface::new(HostOutput::new(1920, 1080, 1.0), TokenMode::dark(), entries);
        surface.open(&mut host).expect("open");
        let contract = surface.last_contract.expect("contract");

        assert_eq!(contract.tiles.len(), COLUMNS * MAX_ROWS);
        assert_eq!(contract.overflow, 23 - COLUMNS * MAX_ROWS);
    }

    #[test]
    fn a_presented_frame_matches_its_placement_and_carries_ink() {
        let (_, host) = opened();
        let (placement, pixels) = host.last_frame(LAUNCHER_NAMESPACE).expect("frame");

        assert_eq!(
            pixels.len(),
            (placement.size.0 * placement.size.1 * 4) as usize
        );
        assert!(pixels.as_chunks::<4>().0.iter().any(|pixel| pixel[3] > 0));
    }

    #[test]
    fn reduced_motion_removes_the_transition_without_changing_the_layout() {
        let (surface, _) = opened();
        let mut reduced = surface.clone();
        reduced.set_token_mode(TokenMode::dark().reduced_motion());

        let full = surface.contract().expect("contract");
        let reduced = reduced.contract().expect("contract");
        assert_eq!(full.tiles, reduced.tiles);
        assert!(full.transition.duration_ms > 0);
        assert_eq!(reduced.transition.duration_ms, 0);
    }

    #[test]
    fn an_output_too_small_for_the_launcher_reports_that_instead_of_overflowing_it() {
        let small =
            LauncherSurface::new(HostOutput::new(120, 90, 1.0), TokenMode::dark(), catalog());
        assert!(matches!(
            small.contract(),
            Err(LauncherSurfaceError::OutputTooSmall(_))
        ));
    }

    #[test]
    fn an_unconfigured_output_refuses_to_produce_a_frame() {
        let surface =
            LauncherSurface::new(HostOutput::new(0, 0, 1.0), TokenMode::dark(), catalog());
        assert!(matches!(
            surface.contract(),
            Err(LauncherSurfaceError::OutputNotConfigured)
        ));
    }
}
