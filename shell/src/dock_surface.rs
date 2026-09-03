//! The native SOL Dock surface.
//!
//! [`crate::launcher::ShellLauncher`] owns the Dock's *state* — which
//! applications are pinned, which are observed running, and what a typed launch
//! or activate request has to pass through. This module owns its *presentation*
//! and its geometry: a bottom-centered `Material::Dock` panel with one tile per
//! entry, plus the hit test that turns a pointer position back into the entry
//! the user aimed at.
//!
//! ## Centering belongs to the compositor
//!
//! The Dock anchors only to the bottom edge and sets no horizontal anchor. A
//! layer surface with neither horizontal edge anchored is centered by the
//! compositor, so the Dock stays centered across a mode change, a scale change,
//! or a hotplug without the Shell recomputing a margin from an output width it
//! would have to re-observe first.
//!
//! ## What a tile shows
//!
//! Application icons are not yet part of the `.app` bundle contract the Shell
//! can read, so a tile carries the initial of its verified display name rather
//! than an icon supplied by the application. That is deliberate rather than
//! temporary shorthand: an unverified image drawn into trusted Shell chrome is
//! how an application impersonates another one, and the tile will take icons
//! from the authenticated catalog or not at all.

use sol_app::AppId;
use sol_design::{
    accessibility::TokenMode, color::Color, material::Material, metrics::ControlMetric,
    radius::Radius, spacing::Spacing, typography::FontStyle,
};
use sol_ui::{AccessibilityNode, AccessibilityState, LogicalSize, SemanticId, SemanticRole};

use crate::{
    launcher::DockItem,
    overlay::LayerShellLayer,
    paint::{Canvas, PixelRect, text_scale_for_height},
    scp_host::{
        DesktopHost, DesktopHostError, HostOutput, LayerAnchor, LayerKeyboard, LayerMargin,
        LayerPlacement,
    },
};

/// Stable SCP namespace of the Dock.
pub const DOCK_NAMESPACE: &str = "sol.dock";

/// Stable identity of the Dock's permanent Launcher entry.
pub const LAUNCHER_TILE_ID: &str = "dock.launcher";

/// What activating a tile asks the Shell to do.
///
/// Deliberately not an action: the Dock reports *what the user aimed at*, and
/// the launcher model decides what may happen next, through the same typed
/// authorization boundary a keyboard or search launch goes through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DockTarget {
    /// The permanent Application Launcher entry.
    Launcher,
    /// A cataloged application tile.
    Application(AppId),
}

/// One laid-out Dock tile.
#[derive(Debug, Clone, PartialEq)]
pub struct DockTile {
    /// Stable identity, used for accessibility and hit testing.
    pub id: String,
    /// What activating this tile targets.
    pub target: DockTarget,
    /// Accessible name.
    pub label: String,
    /// Single-character mark painted in the tile.
    pub mark: String,
    /// Whether the user pinned this application.
    pub pinned: bool,
    /// Whether a trusted session observer reports it running.
    pub running: bool,
    /// Logical rectangle `(x, y, width, height)`, relative to the Dock surface.
    pub rect: (f32, f32, f32, f32),
}

/// A complete Dock frame.
#[derive(Debug, Clone, PartialEq)]
pub struct DockSurfaceContract {
    pub output: HostOutput,
    pub logical_size: LogicalSize,
    pub physical_size: (u32, u32),
    pub placement: LayerPlacement,
    /// Token-resolved surface roles.
    pub material: Material,
    pub background: Color,
    pub border: Color,
    pub accent: Color,
    pub foreground: Color,
    pub radius: Radius,
    pub tile_radius: Radius,
    pub typography: FontStyle,
    pub token_mode: TokenMode,
    /// Tiles in presentation order, Launcher first.
    pub tiles: Vec<DockTile>,
    pub accessibility: AccessibilityNode,
}

/// Errors raised before an invalid Dock frame reaches the compositor.
#[derive(Debug)]
pub enum DockSurfaceError {
    /// The compositor has not reported a usable output extent yet.
    OutputNotConfigured,
    /// The output is too small to place a Dock on.
    OutputTooSmall(LogicalSize),
    /// The frame extent could not be allocated.
    UnpaintableExtent((u32, u32)),
    /// The native host rejected the frame.
    Host(DesktopHostError),
}

impl std::fmt::Display for DockSurfaceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutputNotConfigured => {
                formatter.write_str("no output extent has been configured yet")
            }
            Self::OutputTooSmall(size) => write!(
                formatter,
                "a {}x{} output cannot hold a Dock",
                size.width, size.height
            ),
            Self::UnpaintableExtent((width, height)) => {
                write!(formatter, "cannot paint a {width}x{height} Dock")
            }
            Self::Host(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for DockSurfaceError {}

impl From<DesktopHostError> for DockSurfaceError {
    fn from(error: DesktopHostError) -> Self {
        Self::Host(error)
    }
}

/// Retained Dock surface.
#[derive(Debug, Clone)]
pub struct DockSurface {
    output: HostOutput,
    mode: TokenMode,
    items: Vec<DockItem>,
    focused: Option<AppId>,
    /// Last frame presented, retained for inspection and native hosts.
    pub last_contract: Option<DockSurfaceContract>,
}

impl DockSurface {
    /// Create the Dock for an output and an initial item list.
    #[must_use]
    pub const fn new(output: HostOutput, mode: TokenMode, items: Vec<DockItem>) -> Self {
        Self {
            output,
            mode,
            items,
            focused: None,
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

    /// Replace the Dock's entries from the launcher model.
    pub fn refresh(&mut self, items: Vec<DockItem>) {
        self.items = items;
    }

    /// Record which application the compositor reports as focused.
    pub fn set_focused(&mut self, app_id: Option<AppId>) {
        self.focused = app_id;
    }

    /// Resolve a pointer position, in Dock-surface logical coordinates, to a
    /// tile.
    ///
    /// The hit rectangle is the whole tile cell, not the painted mark: a target
    /// that shrinks with its visual is a target the user misses.
    #[must_use]
    pub fn hit_test(&self, x: f32, y: f32) -> Option<DockTarget> {
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
            .map(|tile| tile.target.clone())
    }

    /// Build the current frame contract without painting it.
    pub fn contract(&self) -> Result<DockSurfaceContract, DockSurfaceError> {
        if !self.output.is_configured() {
            return Err(DockSurfaceError::OutputNotConfigured);
        }

        let padding = Spacing::Md.px();
        let gap = Spacing::Sm.px();
        let indicator = Spacing::Xs.px();
        let cell = ControlMetric::Toolbar.spec().height;

        let mut tiles = Vec::new();
        let mut pen = padding;
        let mut push = |tile: DockTile, pen: &mut f32| {
            *pen += cell + gap;
            tiles.push(tile);
        };

        push(
            DockTile {
                id: LAUNCHER_TILE_ID.to_owned(),
                target: DockTarget::Launcher,
                label: "Application Launcher".to_owned(),
                mark: "::".to_owned(),
                pinned: true,
                running: false,
                rect: (pen, padding, cell, cell),
            },
            &mut pen,
        );

        for item in &self.items {
            let app_id = item.entry.app_id().clone();
            let name = item.entry.identity.display_name().to_owned();
            push(
                DockTile {
                    id: format!("dock.{app_id}"),
                    mark: initial(&name),
                    label: name,
                    pinned: item.pinned,
                    running: item.running,
                    rect: (pen, padding, cell, cell),
                    target: DockTarget::Application(app_id),
                },
                &mut pen,
            );
        }

        // `pen` has one trailing gap past the last tile; the panel ends at the
        // padding instead.
        let width = pen - gap + padding;
        let height = padding * 2.0 + cell + gap + indicator;
        let logical_output = self.output.logical_size();
        if width > logical_output.width || height > logical_output.height {
            return Err(DockSurfaceError::OutputTooSmall(logical_output));
        }

        let physical = (
            self.output.physical(width).max(1),
            self.output.physical(height).max(1),
        );

        Ok(DockSurfaceContract {
            output: self.output,
            logical_size: LogicalSize::new(width, height),
            physical_size: (physical.0 as u32, physical.1 as u32),
            placement: LayerPlacement {
                namespace: DOCK_NAMESPACE.to_owned(),
                layer: LayerShellLayer::Top,
                anchor: LayerAnchor::BOTTOM_CENTER,
                margin: LayerMargin::bottom(self.output.physical(Spacing::Sm.px())),
                size: physical,
                // The Dock floats over content rather than reserving work area:
                // a maximized window runs to the bottom edge behind it, which is
                // what makes optional auto-hide a presentation change instead of
                // a relayout of every window on the output.
                exclusive_zone: 0,
                keyboard: LayerKeyboard::None,
            },
            material: Material::Dock,
            background: Color::Elevated,
            border: Color::Border,
            accent: Color::Accent,
            foreground: Color::TextPrimary,
            radius: Radius::Md,
            tile_radius: Radius::Sm,
            typography: FontStyle::Label,
            token_mode: self.mode,
            accessibility: accessibility_tree(&tiles, self.focused.as_ref()),
            tiles,
        })
    }

    /// Paint and present the Dock.
    pub fn present(&mut self, host: &mut impl DesktopHost) -> Result<(), DockSurfaceError> {
        let contract = self.contract()?;
        let pixels = rasterize(&contract, self.focused.as_ref())?;
        host.present(&contract.placement, &pixels)?;
        self.last_contract = Some(contract);
        Ok(())
    }

    /// Withdraw the Dock, for an auto-hide policy or session teardown.
    pub fn dismiss(&mut self, host: &mut impl DesktopHost) -> Result<(), DockSurfaceError> {
        host.dismiss(DOCK_NAMESPACE)?;
        self.last_contract = None;
        Ok(())
    }
}

/// The mark painted in a tile: the first character of the verified name.
fn initial(name: &str) -> String {
    name.chars()
        .next()
        .map_or_else(|| "?".to_owned(), |first| first.to_uppercase().to_string())
}

fn accessibility_tree(tiles: &[DockTile], focused: Option<&AppId>) -> AccessibilityNode {
    AccessibilityNode {
        id: SemanticId::new("dock-surface"),
        role: SemanticRole::Group,
        label: "SOL Dock".to_owned(),
        value: Some(format!("{} entries", tiles.len())),
        state: AccessibilityState::default(),
        children: tiles
            .iter()
            .map(|tile| AccessibilityNode {
                id: SemanticId::new(tile.id.clone()),
                role: SemanticRole::Button,
                label: tile.label.clone(),
                value: Some(tile_state(tile)),
                state: AccessibilityState {
                    focused: matches!(&tile.target, DockTarget::Application(app_id)
                        if Some(app_id) == focused),
                    selected: tile.running,
                    disabled: false,
                    editable: false,
                },
                children: Vec::new(),
            })
            .collect(),
    }
}

fn tile_state(tile: &DockTile) -> String {
    match (tile.pinned, tile.running) {
        (true, true) => "Pinned, running".to_owned(),
        (true, false) => "Pinned".to_owned(),
        (false, true) => "Running".to_owned(),
        (false, false) => "Available".to_owned(),
    }
}

/// Paint one Dock frame.
fn rasterize(
    contract: &DockSurfaceContract,
    focused: Option<&AppId>,
) -> Result<Vec<u8>, DockSurfaceError> {
    let (width, height) = contract.physical_size;
    let mut canvas =
        Canvas::new(width, height).ok_or(DockSurfaceError::UnpaintableExtent((width, height)))?;
    let mode = contract.token_mode;
    let scale = contract.output.scale;
    let material = mode.material_spec(contract.material);

    // The Dock's translucency comes from the material token, not from a
    // hand-picked alpha: reduced transparency and high contrast resolve the same
    // token to a solid surface with the same geometry.
    let mut background = mode.color(contract.background);
    background.3 *= material.tint_opacity;
    let panel = PixelRect::new(0.0, 0.0, width as f32, height as f32);
    canvas.fill_rounded_rect(panel, contract.radius.px() * scale, background);
    if material.explicit_boundary {
        canvas.fill_rounded_rect(
            panel.inset(scale),
            contract.radius.px() * scale,
            mode.color(contract.background),
        );
    }

    let font = mode.typography(contract.typography);
    let text_scale = text_scale_for_height(font.pixels);

    for tile in &contract.tiles {
        let cell = PixelRect::new(
            tile.rect.0 * scale,
            tile.rect.1 * scale,
            tile.rect.2 * scale,
            tile.rect.3 * scale,
        );
        let is_focused = matches!(&tile.target, DockTarget::Application(app_id)
            if Some(app_id) == focused);
        let fill = if is_focused {
            mode.color(contract.accent)
        } else {
            mode.color(Color::HoverOverlay)
        };
        canvas.fill_rounded_rect(cell, contract.tile_radius.px() * scale, fill);

        let mark_width = Canvas::text_width(text_scale * scale, &tile.mark);
        canvas.draw_text(
            (
                cell.center_x() - mark_width / 2.0,
                cell.center_y() - crate::paint::text_height(text_scale * scale) / 2.0,
            ),
            text_scale * scale,
            mode.color(if is_focused {
                Color::TextOnAccent
            } else {
                contract.foreground
            }),
            &tile.mark,
        );

        if tile.running {
            let dot = Spacing::Xs.px() * scale;
            canvas.fill_rounded_rect(
                PixelRect::new(
                    cell.center_x() - dot / 2.0,
                    cell.y + cell.height + Spacing::Sm.px() * scale / 2.0,
                    dot,
                    dot,
                ),
                dot / 2.0,
                mode.color(contract.accent),
            );
        }
    }

    Ok(canvas.into_pixels())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{launcher::AppCatalogEntry, scp_host::RecordingDesktopHost};
    use sol_app::AppIdentity;

    fn entry(app_id: &str, name: &str) -> AppCatalogEntry {
        AppCatalogEntry::new(
            AppIdentity::new(AppId::parse(app_id).expect("valid app id"), name)
                .expect("valid app identity"),
            Vec::new(),
        )
    }

    fn items() -> Vec<DockItem> {
        vec![
            DockItem {
                entry: entry("org.sol.files", "Files"),
                pinned: true,
                running: false,
            },
            DockItem {
                entry: entry("org.sol.terminal", "Terminal"),
                pinned: false,
                running: true,
            },
        ]
    }

    fn surface() -> DockSurface {
        DockSurface::new(HostOutput::new(1920, 1080, 1.0), TokenMode::dark(), items())
    }

    #[test]
    fn the_dock_anchors_to_the_bottom_and_lets_the_compositor_center_it() {
        let contract = surface().contract().expect("contract");

        assert_eq!(contract.placement.layer, LayerShellLayer::Top);
        assert_eq!(contract.placement.anchor, LayerAnchor::BOTTOM_CENTER);
        assert!(!contract.placement.anchor.left && !contract.placement.anchor.right);
        assert_eq!(contract.placement.margin.bottom, Spacing::Sm.px() as i32);
    }

    #[test]
    fn the_dock_reserves_no_work_area_so_windows_run_under_it() {
        assert_eq!(
            surface()
                .contract()
                .expect("contract")
                .placement
                .exclusive_zone,
            0
        );
    }

    #[test]
    fn the_launcher_entry_is_always_first_and_always_present() {
        let empty = DockSurface::new(
            HostOutput::new(1920, 1080, 1.0),
            TokenMode::dark(),
            Vec::new(),
        );
        let contract = empty.contract().expect("contract");

        assert_eq!(contract.tiles.len(), 1);
        assert_eq!(contract.tiles[0].id, LAUNCHER_TILE_ID);
        assert_eq!(contract.tiles[0].target, DockTarget::Launcher);
    }

    #[test]
    fn tiles_follow_the_launcher_in_model_order_with_their_state_intact() {
        let contract = surface().contract().expect("contract");

        assert_eq!(contract.tiles.len(), 3);
        assert_eq!(contract.tiles[1].label, "Files");
        assert!(contract.tiles[1].pinned && !contract.tiles[1].running);
        assert_eq!(contract.tiles[2].label, "Terminal");
        assert!(!contract.tiles[2].pinned && contract.tiles[2].running);
        assert_eq!(contract.tiles[2].mark, "T");
    }

    #[test]
    fn the_dock_grows_with_its_entries() {
        let narrow = DockSurface::new(
            HostOutput::new(1920, 1080, 1.0),
            TokenMode::dark(),
            Vec::new(),
        )
        .contract()
        .expect("contract")
        .logical_size
        .width;
        let wide = surface().contract().expect("contract").logical_size.width;

        assert!(wide > narrow);
        let cell = ControlMetric::Toolbar.spec().height;
        assert_eq!(wide - narrow, (cell + Spacing::Sm.px()) * 2.0);
    }

    #[test]
    fn a_pointer_inside_a_tile_resolves_to_that_application() {
        let mut host = RecordingDesktopHost::default();
        let mut surface = surface();
        surface.present(&mut host).expect("present");

        let contract = surface.last_contract.clone().expect("contract");
        let files = &contract.tiles[1];
        let target = surface
            .hit_test(files.rect.0 + 1.0, files.rect.1 + 1.0)
            .expect("hit a tile");

        assert_eq!(
            target,
            DockTarget::Application(AppId::parse("org.sol.files").expect("valid"))
        );
        assert_eq!(surface.hit_test(0.0, 0.0), None);
    }

    #[test]
    fn hit_testing_before_the_first_frame_targets_nothing() {
        assert_eq!(surface().hit_test(20.0, 20.0), None);
    }

    #[test]
    fn a_presented_frame_matches_its_placement_and_carries_ink() {
        let mut host = RecordingDesktopHost::default();
        let mut surface = surface();
        surface.present(&mut host).expect("present");

        let (placement, pixels) = host.last_frame(DOCK_NAMESPACE).expect("frame");
        let expected = (placement.size.0 * placement.size.1 * 4) as usize;
        assert_eq!(pixels.len(), expected);
        assert!(
            pixels.as_chunks::<4>().0.iter().any(|pixel| pixel[3] > 0),
            "the Dock must paint something"
        );
        // A rounded panel leaves its outermost corner uncovered.
        assert_eq!(pixels[3], 0, "the Dock's corner is rounded, not square");
    }

    #[test]
    fn reduced_transparency_keeps_the_geometry_and_drops_the_translucency() {
        let fluid = surface();
        let mut solid = surface();
        solid.set_token_mode(TokenMode::dark().reduced_transparency());

        let fluid_contract = fluid.contract().expect("contract");
        let solid_contract = solid.contract().expect("contract");
        assert_eq!(fluid_contract.placement.size, solid_contract.placement.size);
        assert_eq!(fluid_contract.tiles, solid_contract.tiles);

        let center = |contract: &DockSurfaceContract| {
            let pixels = rasterize(contract, None).expect("paint");
            let (width, height) = contract.physical_size;
            let index = ((height / 2) as usize * width as usize + 1) * 4;
            pixels[index + 3]
        };
        assert!(
            center(&solid_contract) > center(&fluid_contract),
            "reduced transparency must resolve the Dock material to a solid surface"
        );
    }

    #[test]
    fn the_focused_application_is_marked_in_the_accessibility_tree() {
        let mut surface = surface();
        surface.set_focused(Some(AppId::parse("org.sol.terminal").expect("valid")));
        let contract = surface.contract().expect("contract");

        let terminal = &contract.accessibility.children[2];
        assert!(terminal.state.focused);
        assert!(!contract.accessibility.children[1].state.focused);
    }

    #[test]
    fn an_output_too_small_for_a_dock_reports_that_instead_of_overflowing_it() {
        let tiny = DockSurface::new(HostOutput::new(24, 24, 1.0), TokenMode::dark(), items());
        assert!(matches!(
            tiny.contract(),
            Err(DockSurfaceError::OutputTooSmall(_))
        ));
    }

    #[test]
    fn an_unconfigured_output_refuses_to_produce_a_frame() {
        let surface = DockSurface::new(HostOutput::new(0, 0, 1.0), TokenMode::dark(), items());
        assert!(matches!(
            surface.contract(),
            Err(DockSurfaceError::OutputNotConfigured)
        ));
    }

    #[test]
    fn dismissing_the_dock_withdraws_the_surface_and_clears_its_hit_targets() {
        let mut host = RecordingDesktopHost::default();
        let mut surface = surface();
        surface.present(&mut host).expect("present");
        surface.dismiss(&mut host).expect("dismiss");

        assert_eq!(host.dismissed, vec![DOCK_NAMESPACE.to_owned()]);
        assert_eq!(surface.hit_test(20.0, 20.0), None);
    }
}
