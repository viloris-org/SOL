//! Renderer-neutral SolUI projection for the Files application.
//!
//! This layer owns transient view state such as the in-folder search query and
//! open context menu. Files' filesystem policy remains in [`FilesApp`]. A
//! platform adapter can render [`FilesSurfaceProjection`] without importing a
//! concrete renderer or duplicating file-operation behavior.

use super::{FileEntry, FileKind, FilesApp, FilesError, TrashItem, TrashStore};
use sol_ui::{
    AccessibilityNode, Button, InteractionTree, SemanticControl, StackItem, Tab, TabBar, TextField,
    Toolbar, ToolbarItem, VStack,
};
use std::error::Error;
use std::fmt;
use std::path::PathBuf;

/// Sidebar locations available without a platform location-provider service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesSidebarLocation {
    /// Reload the active folder.
    CurrentFolder,
    /// Navigate to the active folder's parent when one exists.
    ParentFolder,
}

/// Context actions supported entirely by the local Files core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesContextAction {
    /// Navigate into a directory.
    Open,
    /// Move the selected item into the configured recoverable trash store.
    MoveToTrash,
}

/// One typed interaction received from a renderer adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilesSurfaceEvent {
    /// Select an existing Files tab.
    ActivateTab(usize),
    /// Navigate via one of the local sidebar locations.
    ActivateSidebar(FilesSidebarLocation),
    /// Replace the in-folder search query.
    SetSearchQuery(String),
    /// Select an item and display its context actions.
    OpenContext(PathBuf),
    /// Apply an action from the currently open context menu.
    ActivateContext(FilesContextAction),
    /// Close the current context menu without changing Files state.
    DismissContext,
}

/// Result of a Files surface interaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilesSurfaceOutcome {
    /// A tab became active.
    TabActivated(usize),
    /// A sidebar location was applied.
    SidebarActivated(FilesSidebarLocation),
    /// The in-folder query changed.
    SearchQueryChanged,
    /// A context menu opened for the given visible item.
    ContextOpened(PathBuf),
    /// A directory context action changed the active folder.
    DirectoryOpened(PathBuf),
    /// A context action moved an item into recoverable trash.
    Trashed(TrashItem),
    /// The context menu was dismissed.
    ContextDismissed,
}

/// Failure while translating a surface interaction into a Files core action.
#[derive(Debug)]
pub enum FilesSurfaceError {
    /// The underlying Files core rejected the requested operation.
    Files(FilesError),
    /// No context menu is open for an action that requires one.
    NoContextMenu,
    /// The action is unavailable for the selected item kind.
    UnsupportedContextAction {
        /// Action requested by the renderer.
        action: FilesContextAction,
        /// Item for which the action was requested.
        path: PathBuf,
    },
}

impl fmt::Display for FilesSurfaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Files(error) => error.fmt(formatter),
            Self::NoContextMenu => formatter.write_str("no Files context menu is open"),
            Self::UnsupportedContextAction { action, path } => {
                write!(
                    formatter,
                    "{action:?} is unavailable for {}",
                    path.display()
                )
            }
        }
    }
}

impl Error for FilesSurfaceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Files(error) => Some(error),
            Self::NoContextMenu | Self::UnsupportedContextAction { .. } => None,
        }
    }
}

impl From<FilesError> for FilesSurfaceError {
    fn from(error: FilesError) -> Self {
        Self::Files(error)
    }
}

/// Result returned by a Files surface event.
pub type FilesSurfaceResult<T> = Result<T, FilesSurfaceError>;

/// Sidebar projection in its deterministic display order.
pub struct FilesSidebarProjection {
    /// SolUI vertical layout containing the sidebar buttons in `locations` order.
    pub layout: VStack,
    /// Typed destination for each sidebar button.
    pub locations: Vec<FilesSidebarLocation>,
}

/// Context-menu projection in its deterministic display order.
pub struct FilesContextMenuProjection {
    /// Item to which the menu applies.
    pub target: FileEntry,
    /// SolUI vertical layout containing the action buttons in `actions` order.
    pub layout: VStack,
    /// Typed action for each context-menu button.
    pub actions: Vec<FilesContextAction>,
}

/// Complete renderer-neutral Files view state built from SolUI components.
pub struct FilesSurfaceProjection {
    /// Navigation and common command controls.
    pub toolbar: Toolbar,
    /// One SolUI tab for each core directory tab.
    pub tabs: TabBar,
    /// The local, platform-independent sidebar.
    pub sidebar: FilesSidebarProjection,
    /// The in-folder search field.
    pub search: TextField,
    /// Search-filtered entries to render in the active directory layout.
    pub entries: Vec<FileEntry>,
    /// Context menu for the selected entry, if one is open and still visible.
    pub context_menu: Option<FilesContextMenuProjection>,
}

/// Retained Files view state that is independent of the rendering backend.
#[derive(Debug, Default)]
pub struct FilesSurface {
    search_query: String,
    context_target: Option<PathBuf>,
}

impl FilesSurface {
    /// Create an empty native Files surface state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the current in-folder search query.
    #[must_use]
    pub fn search_query(&self) -> &str {
        &self.search_query
    }

    /// Build SolUI components from the current Files core state.
    #[must_use]
    pub fn project<T: TrashStore>(&self, files: &FilesApp<T>) -> FilesSurfaceProjection {
        let toolbar = Toolbar::new()
            .item(ToolbarItem::Button(Button::new().with_label("Back")))
            .item(ToolbarItem::Button(Button::new().with_label("New tab")))
            .item(ToolbarItem::Button(Button::new().with_label("Refresh")))
            .item(ToolbarItem::Button(
                Button::new().with_label("Command palette"),
            ));

        let tabs = files
            .tabs()
            .iter()
            .enumerate()
            .fold(TabBar::new(), |bar, (index, tab)| {
                let tab = Tab::new(directory_label(&tab.directory));
                let tab = if index == files.active_tab_index() {
                    tab.select()
                } else {
                    tab
                };
                bar.tab(tab)
            });

        let parent_enabled = files.active_tab().directory.parent().is_some();
        let sidebar = FilesSidebarProjection {
            layout: VStack::new()
                .item(StackItem::Button(
                    Button::new().with_label("Current folder"),
                ))
                .item(StackItem::Button(
                    Button::new()
                        .with_label("Parent folder")
                        .enabled(parent_enabled),
                )),
            locations: vec![
                FilesSidebarLocation::CurrentFolder,
                FilesSidebarLocation::ParentFolder,
            ],
        };

        let mut search = TextField::new().with_placeholder("Search this folder");
        search.text.clone_from(&self.search_query);

        FilesSurfaceProjection {
            toolbar,
            tabs,
            sidebar,
            search,
            entries: files.search(&self.search_query),
            context_menu: self.context_menu(files),
        }
    }

    /// Translate a typed renderer event through the existing Files core APIs.
    pub fn handle_event<T: TrashStore>(
        &mut self,
        files: &mut FilesApp<T>,
        event: FilesSurfaceEvent,
    ) -> FilesSurfaceResult<FilesSurfaceOutcome> {
        match event {
            FilesSurfaceEvent::ActivateTab(index) => {
                files.activate_tab(index)?;
                self.context_target = None;
                Ok(FilesSurfaceOutcome::TabActivated(index))
            }
            FilesSurfaceEvent::ActivateSidebar(location) => {
                match location {
                    FilesSidebarLocation::CurrentFolder => files.refresh()?,
                    FilesSidebarLocation::ParentFolder => files.navigate_up()?,
                }
                self.context_target = None;
                Ok(FilesSurfaceOutcome::SidebarActivated(location))
            }
            FilesSurfaceEvent::SetSearchQuery(query) => {
                self.search_query = query;
                Ok(FilesSurfaceOutcome::SearchQueryChanged)
            }
            FilesSurfaceEvent::OpenContext(path) => {
                files.select(&path)?;
                let path = files
                    .active_tab()
                    .cursor
                    .clone()
                    .ok_or(FilesSurfaceError::Files(FilesError::InvalidPath {
                        path,
                        message: "selecting a visible entry did not set the Files cursor",
                    }))?;
                self.context_target = Some(path.clone());
                Ok(FilesSurfaceOutcome::ContextOpened(path))
            }
            FilesSurfaceEvent::ActivateContext(action) => self.activate_context(files, action),
            FilesSurfaceEvent::DismissContext => {
                self.context_target = None;
                Ok(FilesSurfaceOutcome::ContextDismissed)
            }
        }
    }

    /// Build the accessibility projection from the same SolUI controls as [`Self::project`].
    #[must_use]
    pub fn accessibility_tree<T: TrashStore>(&self, files: &FilesApp<T>) -> AccessibilityNode {
        let mut tree = InteractionTree::new("files-surface", "Files");
        tree.push(SemanticControl::button(
            "files.sidebar.current",
            &Button::new().with_label("Current folder"),
        ));
        tree.push(SemanticControl::button(
            "files.sidebar.parent",
            &Button::new()
                .with_label("Parent folder")
                .enabled(files.active_tab().directory.parent().is_some()),
        ));
        let mut search = TextField::new().with_placeholder("Search this folder");
        search.text.clone_from(&self.search_query);
        tree.push(SemanticControl::text_field("files.search", &search));
        for (index, tab) in files.tabs().iter().enumerate() {
            let tab = Tab::new(directory_label(&tab.directory));
            let tab = if index == files.active_tab_index() {
                tab.select()
            } else {
                tab
            };
            tree.push(SemanticControl::tab(format!("files.tab.{index}"), &tab));
        }
        if let Some(menu) = self.context_menu(files) {
            for action in menu.actions {
                tree.push(SemanticControl::button(
                    context_action_id(action),
                    &context_action_button(action),
                ));
            }
        }
        tree.accessibility_tree()
    }

    fn context_menu<T: TrashStore>(
        &self,
        files: &FilesApp<T>,
    ) -> Option<FilesContextMenuProjection> {
        let target = self.context_target.as_ref()?;
        let target = files
            .active_tab()
            .entries
            .iter()
            .find(|entry| &entry.path == target)?
            .clone();
        let mut layout = VStack::new();
        let mut actions = Vec::new();
        if target.kind == FileKind::Directory {
            layout = layout.item(StackItem::Button(context_action_button(
                FilesContextAction::Open,
            )));
            actions.push(FilesContextAction::Open);
        }
        layout = layout.item(StackItem::Button(context_action_button(
            FilesContextAction::MoveToTrash,
        )));
        actions.push(FilesContextAction::MoveToTrash);
        Some(FilesContextMenuProjection {
            target,
            layout,
            actions,
        })
    }

    fn activate_context<T: TrashStore>(
        &mut self,
        files: &mut FilesApp<T>,
        action: FilesContextAction,
    ) -> FilesSurfaceResult<FilesSurfaceOutcome> {
        let path = self
            .context_target
            .clone()
            .ok_or(FilesSurfaceError::NoContextMenu)?;
        let kind = files
            .active_tab()
            .entries
            .iter()
            .find(|entry| entry.path == path)
            .map(|entry| entry.kind)
            .ok_or_else(|| {
                FilesSurfaceError::Files(FilesError::InvalidPath {
                    path: path.clone(),
                    message: "the context item is no longer visible",
                })
            })?;
        match action {
            FilesContextAction::Open if kind == FileKind::Directory => {
                files.navigate_to(&path)?;
                self.context_target = None;
                Ok(FilesSurfaceOutcome::DirectoryOpened(path))
            }
            FilesContextAction::Open => {
                Err(FilesSurfaceError::UnsupportedContextAction { action, path })
            }
            FilesContextAction::MoveToTrash => {
                let item = files.delete_to_trash(&path)?;
                self.context_target = None;
                Ok(FilesSurfaceOutcome::Trashed(item))
            }
        }
    }
}

fn directory_label(path: &std::path::Path) -> String {
    path.file_name()
        .filter(|name| !name.is_empty())
        .map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        )
}

fn context_action_button(action: FilesContextAction) -> Button {
    match action {
        FilesContextAction::Open => Button::new().with_label("Open"),
        FilesContextAction::MoveToTrash => Button::new().with_label("Move to Trash"),
    }
}

fn context_action_id(action: FilesContextAction) -> &'static str {
    match action {
        FilesContextAction::Open => "files.context.open",
        FilesContextAction::MoveToTrash => "files.context.move-to-trash",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let nonce = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "sol-files-surface-test-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir_all(&root).expect("fixture root should be created");
            Self { root }
        }

        fn path(&self, name: &str) -> PathBuf {
            self.root.join(name)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn app(fixture: &Fixture) -> FilesApp<super::super::DirectoryTrash> {
        FilesApp::new(
            &fixture.root,
            super::super::DirectoryTrash::new(fixture.path(".trash")),
        )
        .expect("fixture directory should open")
    }

    #[test]
    fn projection_uses_solui_components_for_tabs_sidebar_search_and_context() {
        let fixture = Fixture::new();
        fs::write(fixture.path("alpha.txt"), "a").expect("fixture file should be written");
        fs::write(fixture.path("bravo.txt"), "b").expect("fixture file should be written");
        fs::create_dir(fixture.path("documents")).expect("fixture directory should be created");
        let mut files = app(&fixture);
        let mut surface = FilesSurface::new();

        assert_eq!(
            surface
                .handle_event(
                    &mut files,
                    FilesSurfaceEvent::SetSearchQuery("ALP".to_owned()),
                )
                .unwrap(),
            FilesSurfaceOutcome::SearchQueryChanged
        );
        surface
            .handle_event(
                &mut files,
                FilesSurfaceEvent::OpenContext(fixture.path("documents")),
            )
            .unwrap();

        let projection = surface.project(&files);
        assert_eq!(projection.toolbar.items.len(), 4);
        assert_eq!(projection.tabs.tabs.len(), 1);
        assert!(projection.tabs.tabs[0].selected);
        assert_eq!(projection.sidebar.locations.len(), 2);
        assert!(matches!(
            projection.sidebar.layout.children[0],
            StackItem::Button(_)
        ));
        assert_eq!(projection.search.text, "ALP");
        assert_eq!(
            projection
                .entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            vec!["alpha.txt"]
        );
        let context = projection
            .context_menu
            .expect("directory menu should project");
        assert_eq!(context.target.name, "documents");
        assert_eq!(
            context.actions,
            vec![FilesContextAction::Open, FilesContextAction::MoveToTrash]
        );
        assert_eq!(context.layout.children.len(), 2);

        let tree = surface.accessibility_tree(&files);
        assert!(
            tree.children
                .iter()
                .any(|node| node.id.as_str() == "files.search")
        );
        assert!(
            tree.children
                .iter()
                .any(|node| node.id.as_str() == "files.context.move-to-trash")
        );
    }

    #[test]
    fn surface_events_delegate_tab_sidebar_and_context_actions_to_files_core() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.path("documents")).expect("fixture directory should be created");
        fs::write(fixture.path("trash-me.txt"), "data").expect("fixture file should be written");
        let mut files = app(&fixture);
        let mut surface = FilesSurface::new();

        let documents = fixture.path("documents");
        files.open_tab(&documents).unwrap();
        assert_eq!(files.active_tab_index(), 1);
        assert_eq!(
            surface
                .handle_event(&mut files, FilesSurfaceEvent::ActivateTab(0))
                .unwrap(),
            FilesSurfaceOutcome::TabActivated(0)
        );
        assert_eq!(files.active_tab_index(), 0);
        let projection = surface.project(&files);
        assert_eq!(projection.tabs.tabs.len(), 2);
        assert_eq!(projection.tabs.tabs[1].label, "documents");
        assert!(projection.tabs.tabs[0].selected);
        assert!(!projection.tabs.tabs[1].selected);

        surface
            .handle_event(
                &mut files,
                FilesSurfaceEvent::OpenContext(documents.clone()),
            )
            .unwrap();
        assert_eq!(
            surface
                .handle_event(
                    &mut files,
                    FilesSurfaceEvent::ActivateContext(FilesContextAction::Open),
                )
                .unwrap(),
            FilesSurfaceOutcome::DirectoryOpened(documents.clone())
        );
        assert_eq!(files.active_tab().directory, documents);
        surface
            .handle_event(
                &mut files,
                FilesSurfaceEvent::ActivateSidebar(FilesSidebarLocation::ParentFolder),
            )
            .unwrap();
        assert_eq!(files.active_tab().directory, fixture.root);

        let file = fixture.path("trash-me.txt");
        surface
            .handle_event(&mut files, FilesSurfaceEvent::OpenContext(file.clone()))
            .unwrap();
        let menu = surface.project(&files).context_menu.unwrap();
        assert_eq!(menu.actions, vec![FilesContextAction::MoveToTrash]);
        let FilesSurfaceOutcome::Trashed(item) = surface
            .handle_event(
                &mut files,
                FilesSurfaceEvent::ActivateContext(FilesContextAction::MoveToTrash),
            )
            .unwrap()
        else {
            panic!("trash action should return its recoverable item");
        };
        assert_eq!(item.original_path, file);
        assert!(!file.exists());
        assert!(item.trashed_path.exists());
    }
}
