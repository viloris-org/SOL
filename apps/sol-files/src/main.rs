//! Renderer-neutral core for the first-party Files application.
//!
//! The app owns file-manager policy (selection, ordering, commands, and safe
//! file operations). A future SolUI native surface consumes this model; it is
//! deliberately not tied to Slint or another concrete renderer.

mod preview;
mod surface;

pub use preview::{FilePreview, ImageThumbnail, PreviewKind, local_preview};
pub use surface::{
    FilesContextAction, FilesContextMenuProjection, FilesSidebarLocation, FilesSidebarProjection,
    FilesSurface, FilesSurfaceError, FilesSurfaceEvent, FilesSurfaceOutcome,
    FilesSurfaceProjection, FilesSurfaceResult,
};

use sol_app::{App, AppId};
use sol_design::{color::Color, motion::Motion, spacing::Spacing};
use sol_ui::{
    AccessibilityNode, Button, CommandPalette, CommandPaletteOutcome, InteractionTree, Key,
    PaletteCommand, SemanticControl, TextField,
};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

/// A filesystem entry displayed by Files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// Base filename used by list and grid projections.
    pub name: String,
    /// Full path addressed by file operations.
    pub path: PathBuf,
    /// Semantic entry kind used for sorting and navigation.
    pub kind: FileKind,
    /// Size in bytes for regular files (zero for directories).
    pub size: u64,
    /// Last-modified time, if the platform reports it.
    pub modified: Option<SystemTime>,
}

/// The kind of file-system object known to the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FileKind {
    /// A regular file.
    File,
    /// A directory that may be navigated into.
    Directory,
    /// A symbolic link. Files does not follow it when reading a directory.
    Symlink,
    /// A platform-specific object outside the primary kinds.
    Other,
}

impl FileKind {
    fn from_file_type(file_type: fs::FileType) -> Self {
        if file_type.is_dir() {
            Self::Directory
        } else if file_type.is_file() {
            Self::File
        } else if file_type.is_symlink() {
            Self::Symlink
        } else {
            Self::Other
        }
    }
}

/// Visual organization exposed to a future native Files surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryLayout {
    /// Rows with metadata columns.
    List,
    /// Icon-oriented tiles.
    Grid,
}

/// Metadata field used to order a directory projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortField {
    /// Case-insensitive filename ordering.
    Name,
    /// Directories, files, links, then other entries.
    Kind,
    /// Newest modification first.
    Modified,
    /// Largest entry first.
    Size,
}

/// Sort configuration retained by each Files tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortOrder {
    /// Field used for ordering.
    pub field: SortField,
    /// Whether ordering is reversed after the field's natural direction.
    pub descending: bool,
}

impl Default for SortOrder {
    fn default() -> Self {
        Self {
            field: SortField::Name,
            descending: false,
        }
    }
}

/// A navigable directory tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryTab {
    /// Canonical directory currently displayed by the tab.
    pub directory: PathBuf,
    /// Entries in the directory after applying the tab's sort order.
    pub entries: Vec<FileEntry>,
    /// Layout selected for this tab.
    pub layout: DirectoryLayout,
    /// Current entry ordering.
    pub sort: SortOrder,
    /// Multi-selection retained as paths rather than volatile row indices.
    pub selection: BTreeSet<PathBuf>,
    /// Focused entry for deterministic keyboard navigation.
    pub cursor: Option<PathBuf>,
}

/// A recoverable deletion record returned by a [`TrashStore`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashItem {
    /// Original path to restore to.
    pub original_path: PathBuf,
    /// Opaque location managed by the trash implementation.
    pub trashed_path: PathBuf,
}

/// Boundary for recoverable deletion.
///
/// Platform portals or a desktop trash service can implement this trait without
/// changing Files' command or selection APIs.
pub trait TrashStore {
    /// Move `path` into recoverable storage and retain its origin.
    fn trash(&self, path: &Path) -> FilesResult<TrashItem>;

    /// Restore a previously trashed item to its recorded original location.
    fn restore(&self, item: &TrashItem) -> FilesResult<()>;
}

/// A simple directory-backed trash implementation for local filesystems.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryTrash {
    root: PathBuf,
}

impl DirectoryTrash {
    /// Create a trash store rooted at an app-selected private directory.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn destination_for(&self, path: &Path) -> FilesResult<PathBuf> {
        let name = path.file_name().ok_or(FilesError::InvalidPath {
            path: path.to_path_buf(),
            message: "a filesystem root cannot be moved to trash",
        })?;
        let mut destination = self.root.join(name);
        let mut duplicate = 1_u32;
        while destination.exists() {
            let suffix = format!(".{}", duplicate);
            destination = self
                .root
                .join(format!("{}{}", name.to_string_lossy(), suffix));
            duplicate = duplicate.checked_add(1).ok_or(FilesError::InvalidPath {
                path: path.to_path_buf(),
                message: "too many name collisions in trash",
            })?;
        }
        Ok(destination)
    }
}

impl TrashStore for DirectoryTrash {
    fn trash(&self, path: &Path) -> FilesResult<TrashItem> {
        fs::create_dir_all(&self.root)
            .map_err(|source| FilesError::io("create trash", &self.root, source))?;
        let destination = self.destination_for(path)?;
        fs::rename(path, &destination)
            .map_err(|source| FilesError::io("move to trash", path, source))?;
        Ok(TrashItem {
            original_path: path.to_path_buf(),
            trashed_path: destination,
        })
    }

    fn restore(&self, item: &TrashItem) -> FilesResult<()> {
        if item.original_path.exists() {
            return Err(FilesError::Conflict {
                path: item.original_path.clone(),
                message: "cannot restore over an existing item",
            });
        }
        if let Some(parent) = item.original_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|source| FilesError::io("create restore directory", parent, source))?;
        }
        fs::rename(&item.trashed_path, &item.original_path)
            .map_err(|source| FilesError::io("restore from trash", &item.trashed_path, source))
    }
}

/// Requested operation for a drag/drop payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropOperation {
    /// Duplicate every source under the target directory.
    Copy,
    /// Relocate every source under the target directory.
    Move,
}

/// Renderer-neutral drag/drop contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropRequest {
    /// Files or directories supplied by the drag source.
    pub sources: Vec<PathBuf>,
    /// Directory accepting the drop.
    pub target: PathBuf,
    /// Chosen operation after modifier/policy resolution.
    pub operation: DropOperation,
}

/// Result for each item in a successful drop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropResult {
    /// Source item accepted by the target.
    pub source: PathBuf,
    /// Destination path created or moved to.
    pub destination: PathBuf,
}

/// Shared command-palette metadata for Files actions.
pub type FilesCommand = PaletteCommand;

const FILES_COMMANDS: [PaletteCommand; 7] = [
    PaletteCommand {
        id: "view.list",
        title: "Use list view",
    },
    PaletteCommand {
        id: "view.grid",
        title: "Use grid view",
    },
    PaletteCommand {
        id: "sort.name",
        title: "Sort by name",
    },
    PaletteCommand {
        id: "sort.modified",
        title: "Sort by modified time",
    },
    PaletteCommand {
        id: "selection.all",
        title: "Select all",
    },
    PaletteCommand {
        id: "directory.refresh",
        title: "Refresh folder",
    },
    PaletteCommand {
        id: "tab.new",
        title: "Open current folder in new tab",
    },
];

/// Keyboard input interpreted by the directory model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesKey {
    /// Move the cursor to the previous entry.
    ArrowUp,
    /// Move the cursor to the next entry.
    ArrowDown,
    /// Extend selection while moving the cursor backward.
    ShiftArrowUp,
    /// Extend selection while moving the cursor forward.
    ShiftArrowDown,
    /// Toggle the cursor item without clearing other selections.
    Space,
    /// Select every visible entry.
    SelectAll,
    /// Clear the current multi-selection.
    Escape,
    /// Navigate into the focused directory.
    Enter,
    /// Navigate to the current directory's parent.
    Back,
}

/// Kind of expected file-manager failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesErrorKind {
    /// The operation was denied by filesystem permissions or policy.
    PermissionDenied,
    /// A path did not exist when the operation ran.
    NotFound,
    /// A destination already exists or would create an invalid hierarchy.
    Conflict,
    /// User-provided path or command data was invalid.
    InvalidInput,
    /// Another filesystem failure occurred.
    Io,
}

/// Error returned by Files operations.
#[derive(Debug)]
pub enum FilesError {
    /// A filesystem call failed.
    Io {
        /// Operation attempted.
        operation: &'static str,
        /// Affected path.
        path: PathBuf,
        /// Normalized class for UI and accessibility feedback.
        kind: FilesErrorKind,
        /// Original platform error.
        source: io::Error,
    },
    /// A path violates Files' safe operation contract.
    InvalidPath {
        /// Invalid path.
        path: PathBuf,
        /// Explanation suitable for user feedback.
        message: &'static str,
    },
    /// An operation would overwrite an existing item or recurse into itself.
    Conflict {
        /// Conflicting path.
        path: PathBuf,
        /// Explanation suitable for user feedback.
        message: &'static str,
    },
    /// The command palette received an unknown command identifier.
    UnknownCommand(String),
    /// Files application identity could not be initialized.
    AppIdentity(String),
}

impl FilesError {
    fn io(operation: &'static str, path: &Path, source: io::Error) -> Self {
        let kind = match source.kind() {
            io::ErrorKind::PermissionDenied => FilesErrorKind::PermissionDenied,
            io::ErrorKind::NotFound => FilesErrorKind::NotFound,
            io::ErrorKind::AlreadyExists => FilesErrorKind::Conflict,
            _ => FilesErrorKind::Io,
        };
        Self::Io {
            operation,
            path: path.to_path_buf(),
            kind,
            source,
        }
    }

    /// Return the normalized category consumed by a native UI.
    #[must_use]
    pub const fn kind(&self) -> FilesErrorKind {
        match self {
            Self::Io { kind, .. } => *kind,
            Self::InvalidPath { .. } | Self::UnknownCommand(_) | Self::AppIdentity(_) => {
                FilesErrorKind::InvalidInput
            }
            Self::Conflict { .. } => FilesErrorKind::Conflict,
        }
    }
}

impl fmt::Display for FilesError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io {
                operation,
                path,
                source,
                ..
            } => write!(formatter, "cannot {operation} {}: {source}", path.display()),
            Self::InvalidPath { path, message } | Self::Conflict { path, message } => {
                write!(formatter, "{}: {message}", path.display())
            }
            Self::UnknownCommand(command) => write!(formatter, "unknown Files command: {command}"),
            Self::AppIdentity(error) => {
                write!(formatter, "invalid Files application identity: {error}")
            }
        }
    }
}

impl Error for FilesError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Result returned by Files operations.
pub type FilesResult<T> = Result<T, FilesError>;

/// First-party Files application state and local filesystem policy.
pub struct FilesApp<T: TrashStore> {
    /// SolKit application lifecycle and durable application identity.
    pub app: App,
    tabs: Vec<DirectoryTab>,
    active_tab: usize,
    trash: T,
    semantic_tree: InteractionTree,
    palette: CommandPalette,
}

impl<T: TrashStore> FilesApp<T> {
    /// Create Files with one tab rooted at `directory`.
    pub fn new(directory: impl AsRef<Path>, trash: T) -> FilesResult<Self> {
        let directory = canonical_directory(directory.as_ref())?;
        let id = AppId::parse("org.sol.files")
            .map_err(|error| FilesError::AppIdentity(error.to_string()))?;
        let mut app = App::new(id);
        app.add_window(sol_app::AppWindow::new("Files"));
        let mut semantic_tree = InteractionTree::new("files", "Files");
        semantic_tree.push(SemanticControl::text_field(
            "address",
            &TextField::new().with_placeholder("Location"),
        ));
        semantic_tree.push(SemanticControl::button(
            "new-tab",
            &Button::new().with_label("New tab"),
        ));
        semantic_tree.push(SemanticControl::button(
            "command-palette",
            &Button::new().with_label("Command palette"),
        ));
        let tab = DirectoryTab {
            entries: read_directory(&directory, SortOrder::default())?,
            directory,
            layout: DirectoryLayout::List,
            sort: SortOrder::default(),
            selection: BTreeSet::new(),
            cursor: None,
        };
        Ok(Self {
            app,
            tabs: vec![tab],
            active_tab: 0,
            trash,
            semantic_tree,
            palette: CommandPalette::new(&FILES_COMMANDS),
        })
    }

    /// Return all open directory tabs.
    #[must_use]
    pub fn tabs(&self) -> &[DirectoryTab] {
        &self.tabs
    }

    /// Return the index of the tab currently shown by the native surface.
    #[must_use]
    pub const fn active_tab_index(&self) -> usize {
        self.active_tab
    }

    /// Return the active tab's model.
    #[must_use]
    pub fn active_tab(&self) -> &DirectoryTab {
        &self.tabs[self.active_tab]
    }

    /// Return a bounded local preview for the keyboard-selected entry.
    pub fn selected_preview(&self) -> FilesResult<Option<FilePreview>> {
        self.active_tab()
            .cursor
            .as_ref()
            .and_then(|path| {
                self.active_tab()
                    .entries
                    .iter()
                    .find(|entry| entry.path == *path)
            })
            .map(local_preview)
            .transpose()
    }

    fn active_tab_mut(&mut self) -> &mut DirectoryTab {
        &mut self.tabs[self.active_tab]
    }

    /// Open `directory` in a new tab and make it active.
    pub fn open_tab(&mut self, directory: impl AsRef<Path>) -> FilesResult<usize> {
        let directory = canonical_directory(directory.as_ref())?;
        let tab = DirectoryTab {
            entries: read_directory(&directory, SortOrder::default())?,
            directory,
            layout: DirectoryLayout::List,
            sort: SortOrder::default(),
            selection: BTreeSet::new(),
            cursor: None,
        };
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        Ok(self.active_tab)
    }

    /// Make an existing tab active.
    pub fn activate_tab(&mut self, index: usize) -> FilesResult<()> {
        if index >= self.tabs.len() {
            return Err(FilesError::InvalidPath {
                path: PathBuf::from(index.to_string()),
                message: "tab index does not exist",
            });
        }
        self.active_tab = index;
        Ok(())
    }

    /// Close a tab while retaining at least one navigable directory.
    pub fn close_tab(&mut self, index: usize) -> FilesResult<()> {
        if self.tabs.len() == 1 {
            return Err(FilesError::Conflict {
                path: self.active_tab().directory.clone(),
                message: "Files always keeps one directory tab open",
            });
        }
        if index >= self.tabs.len() {
            return Err(FilesError::InvalidPath {
                path: PathBuf::from(index.to_string()),
                message: "tab index does not exist",
            });
        }
        self.tabs.remove(index);
        self.active_tab = self.active_tab.min(self.tabs.len() - 1);
        Ok(())
    }

    /// Navigate the active tab to a typed address.
    pub fn navigate_to(&mut self, directory: impl AsRef<Path>) -> FilesResult<()> {
        let directory = canonical_directory(directory.as_ref())?;
        let tab = self.active_tab_mut();
        tab.entries = read_directory(&directory, tab.sort)?;
        tab.directory = directory;
        tab.selection.clear();
        tab.cursor = None;
        Ok(())
    }

    /// Navigate to the active directory's parent, if it has one.
    pub fn navigate_up(&mut self) -> FilesResult<()> {
        let parent = self.active_tab().directory.parent().map(Path::to_path_buf);
        if let Some(parent) = parent {
            self.navigate_to(parent)?;
        }
        Ok(())
    }

    /// Return the path components displayed as address breadcrumbs.
    #[must_use]
    pub fn breadcrumbs(&self) -> Vec<PathBuf> {
        let mut breadcrumb = PathBuf::new();
        self.active_tab()
            .directory
            .components()
            .filter_map(|component| match component {
                Component::Prefix(prefix) => {
                    breadcrumb.push(prefix.as_os_str());
                    Some(breadcrumb.clone())
                }
                Component::RootDir => {
                    breadcrumb.push(component.as_os_str());
                    Some(breadcrumb.clone())
                }
                Component::Normal(name) => {
                    breadcrumb.push(name);
                    Some(breadcrumb.clone())
                }
                Component::CurDir | Component::ParentDir => None,
            })
            .collect()
    }

    /// Set the visual directory organization without changing entry identity.
    pub fn set_layout(&mut self, layout: DirectoryLayout) {
        self.active_tab_mut().layout = layout;
    }

    /// Re-sort the active tab and preserve selected paths that still exist.
    pub fn set_sort(&mut self, sort: SortOrder) {
        let tab = self.active_tab_mut();
        tab.sort = sort;
        sort_entries(&mut tab.entries, sort);
    }

    /// Reread the active directory after a filesystem mutation or external change.
    pub fn refresh(&mut self) -> FilesResult<()> {
        let tab = self.active_tab_mut();
        tab.entries = read_directory(&tab.directory, tab.sort)?;
        let visible: BTreeSet<_> = tab.entries.iter().map(|entry| entry.path.clone()).collect();
        tab.selection.retain(|path| visible.contains(path));
        if tab
            .cursor
            .as_ref()
            .is_some_and(|path| !visible.contains(path))
        {
            tab.cursor = None;
        }
        Ok(())
    }

    /// Return visible entries whose names match a case-insensitive search.
    ///
    /// Search is intentionally a pure directory-model projection; a later
    /// indexer can provide cross-location results without changing selection
    /// or rendering contracts.
    #[must_use]
    pub fn search(&self, query: &str) -> Vec<FileEntry> {
        let query = query.to_ascii_lowercase();
        self.active_tab()
            .entries
            .iter()
            .filter(|entry| query.is_empty() || entry.name.to_ascii_lowercase().contains(&query))
            .cloned()
            .collect()
    }

    /// Select exactly one visible entry.
    pub fn select(&mut self, path: impl AsRef<Path>) -> FilesResult<()> {
        let path = self.visible_path(path.as_ref())?;
        let tab = self.active_tab_mut();
        tab.selection.clear();
        tab.selection.insert(path.clone());
        tab.cursor = Some(path);
        Ok(())
    }

    /// Toggle a visible entry in the multi-selection.
    pub fn toggle_selection(&mut self, path: impl AsRef<Path>) -> FilesResult<()> {
        let path = self.visible_path(path.as_ref())?;
        let tab = self.active_tab_mut();
        if !tab.selection.insert(path.clone()) {
            tab.selection.remove(&path);
        }
        tab.cursor = Some(path);
        Ok(())
    }

    /// Select every visible entry.
    pub fn select_all(&mut self) {
        let tab = self.active_tab_mut();
        tab.selection = tab.entries.iter().map(|entry| entry.path.clone()).collect();
        tab.cursor = tab.entries.first().map(|entry| entry.path.clone());
    }

    /// Clear selection and focus cursor.
    pub fn clear_selection(&mut self) {
        let tab = self.active_tab_mut();
        tab.selection.clear();
        tab.cursor = None;
    }

    /// Apply directory-model keyboard navigation.
    pub fn handle_key(&mut self, key: FilesKey) -> FilesResult<()> {
        match key {
            FilesKey::ArrowUp | FilesKey::ShiftArrowUp => {
                self.move_cursor(-1, matches!(key, FilesKey::ShiftArrowUp))
            }
            FilesKey::ArrowDown | FilesKey::ShiftArrowDown => {
                self.move_cursor(1, matches!(key, FilesKey::ShiftArrowDown))
            }
            FilesKey::Space => {
                if let Some(cursor) = self.active_tab().cursor.clone() {
                    self.toggle_selection(cursor)?;
                }
                Ok(())
            }
            FilesKey::SelectAll => {
                self.select_all();
                Ok(())
            }
            FilesKey::Escape => {
                self.clear_selection();
                Ok(())
            }
            FilesKey::Enter => {
                let cursor = self.active_tab().cursor.clone();
                if let Some(path) = cursor
                    && self.entry(&path)?.kind == FileKind::Directory
                {
                    self.navigate_to(path)?;
                }
                Ok(())
            }
            FilesKey::Back => self.navigate_up(),
        }
    }

    /// Copy one local item into a target directory and refresh a visible source tab.
    pub fn copy(
        &mut self,
        source: impl AsRef<Path>,
        target_directory: impl AsRef<Path>,
    ) -> FilesResult<PathBuf> {
        let source = self.require_existing(source.as_ref())?;
        let target = canonical_directory(target_directory.as_ref())?;
        let destination = destination_path(&source, &target)?;
        ensure_not_recursive_directory(&source, &destination)?;
        copy_recursively(&source, &destination)?;
        self.refresh_if_visible(&source)?;
        Ok(destination)
    }

    /// Move one local item into a target directory and refresh a visible source tab.
    pub fn move_to(
        &mut self,
        source: impl AsRef<Path>,
        target_directory: impl AsRef<Path>,
    ) -> FilesResult<PathBuf> {
        let source = self.require_existing(source.as_ref())?;
        let target = canonical_directory(target_directory.as_ref())?;
        let destination = destination_path(&source, &target)?;
        ensure_not_recursive_directory(&source, &destination)?;
        fs::rename(&source, &destination)
            .map_err(|error| FilesError::io("move", &source, error))?;
        self.refresh_if_visible(&source)?;
        Ok(destination)
    }

    /// Rename a local item without allowing path traversal as a new name.
    pub fn rename(&mut self, source: impl AsRef<Path>, new_name: &str) -> FilesResult<PathBuf> {
        if !is_plain_name(new_name) {
            return Err(FilesError::InvalidPath {
                path: PathBuf::from(new_name),
                message: "a new name must be a single non-empty filename",
            });
        }
        let source = self.require_existing(source.as_ref())?;
        let parent = source.parent().ok_or(FilesError::InvalidPath {
            path: source.clone(),
            message: "a filesystem root cannot be renamed",
        })?;
        let destination = parent.join(new_name);
        if destination.exists() {
            return Err(FilesError::Conflict {
                path: destination,
                message: "an item with that name already exists",
            });
        }
        fs::rename(&source, &destination)
            .map_err(|error| FilesError::io("rename", &source, error))?;
        self.refresh_if_visible(&source)?;
        Ok(destination)
    }

    /// Send an item to recoverable trash and refresh the directory projection.
    pub fn delete_to_trash(&mut self, path: impl AsRef<Path>) -> FilesResult<TrashItem> {
        let path = self.require_existing(path.as_ref())?;
        let item = self.trash.trash(&path)?;
        self.refresh_if_visible(&path)?;
        Ok(item)
    }

    /// Restore a previously deleted item through the configured trash contract.
    pub fn restore_from_trash(&mut self, item: &TrashItem) -> FilesResult<()> {
        self.trash.restore(item)?;
        self.refresh_if_visible(&item.original_path)
    }

    /// Apply a renderer-provided drag/drop request through Files' safe operation boundary.
    pub fn perform_drop(&mut self, request: DropRequest) -> FilesResult<Vec<DropResult>> {
        if request.sources.is_empty() {
            return Err(FilesError::InvalidPath {
                path: request.target,
                message: "a drop requires at least one source",
            });
        }
        let mut results = Vec::with_capacity(request.sources.len());
        for source in request.sources {
            let destination = match request.operation {
                DropOperation::Copy => self.copy(&source, &request.target)?,
                DropOperation::Move => self.move_to(&source, &request.target)?,
            };
            results.push(DropResult {
                source,
                destination,
            });
        }
        self.refresh()?;
        Ok(results)
    }

    /// Return command-palette entries matching `query`.
    #[must_use]
    pub fn commands(query: &str) -> Vec<FilesCommand> {
        CommandPalette::filter(&FILES_COMMANDS, query)
    }

    /// Execute a command-palette action that has no renderer-specific behavior.
    pub fn execute_command(&mut self, id: &str) -> FilesResult<()> {
        match id {
            "view.list" => self.set_layout(DirectoryLayout::List),
            "view.grid" => self.set_layout(DirectoryLayout::Grid),
            "sort.name" => self.set_sort(SortOrder::default()),
            "sort.modified" => self.set_sort(SortOrder {
                field: SortField::Modified,
                descending: false,
            }),
            "selection.all" => self.select_all(),
            "directory.refresh" => return self.refresh(),
            "tab.new" => {
                let directory = self.active_tab().directory.clone();
                self.open_tab(directory)?;
            }
            _ => return Err(FilesError::UnknownCommand(id.to_owned())),
        }
        Ok(())
    }

    /// Route the shared command-palette key contract and execute activated commands.
    pub fn handle_command_palette_key(&mut self, key: Key) -> FilesResult<CommandPaletteOutcome> {
        let outcome = self.palette.handle_key(key);
        if let CommandPaletteOutcome::Execute(id) = outcome {
            self.execute_command(id)?;
            Ok(CommandPaletteOutcome::Execute(id))
        } else {
            Ok(outcome)
        }
    }

    /// Renderer-independent semantic projection for accessibility bridges.
    #[must_use]
    pub fn accessibility_tree(&self) -> AccessibilityNode {
        self.semantic_tree.accessibility_tree()
    }

    /// Return the dedicated accessibility projection for the transient palette.
    #[must_use]
    pub fn command_palette_accessibility_tree(&self) -> AccessibilityNode {
        self.palette.accessibility_tree()
    }

    /// Token-only visual policy for the Files chrome.
    #[must_use]
    pub fn chrome_tokens(&self) -> (Color, Spacing, Motion) {
        let button = Button::new().with_label("Command palette");
        let tokens = button.visual_tokens();
        (tokens.background, tokens.padding, tokens.motion)
    }

    fn visible_path(&self, path: &Path) -> FilesResult<PathBuf> {
        let path = self.require_existing(path)?;
        if self
            .active_tab()
            .entries
            .iter()
            .any(|entry| entry.path == path)
        {
            Ok(path)
        } else {
            Err(FilesError::InvalidPath {
                path,
                message: "the item is not visible in the active directory",
            })
        }
    }

    fn require_existing(&self, path: &Path) -> FilesResult<PathBuf> {
        fs::canonicalize(path).map_err(|error| FilesError::io("resolve path", path, error))
    }

    fn entry(&self, path: &Path) -> FilesResult<&FileEntry> {
        self.active_tab()
            .entries
            .iter()
            .find(|entry| entry.path == path)
            .ok_or(FilesError::InvalidPath {
                path: path.to_path_buf(),
                message: "the item is not visible in the active directory",
            })
    }

    fn move_cursor(&mut self, direction: isize, extend: bool) -> FilesResult<()> {
        let tab = self.active_tab_mut();
        if tab.entries.is_empty() {
            return Ok(());
        }
        let current = tab
            .cursor
            .as_ref()
            .and_then(|path| tab.entries.iter().position(|entry| entry.path == *path));
        let index = if let Some(current) = current {
            if direction < 0 {
                current.saturating_sub(1)
            } else {
                current.saturating_add(1).min(tab.entries.len() - 1)
            }
        } else if direction < 0 {
            tab.entries.len() - 1
        } else {
            0
        };
        let path = tab.entries[index].path.clone();
        if !extend {
            tab.selection.clear();
        }
        tab.selection.insert(path.clone());
        tab.cursor = Some(path);
        Ok(())
    }

    fn refresh_if_visible(&mut self, path: &Path) -> FilesResult<()> {
        if path.parent() == Some(self.active_tab().directory.as_path()) {
            self.refresh()?;
        }
        Ok(())
    }
}

fn canonical_directory(directory: &Path) -> FilesResult<PathBuf> {
    let directory = fs::canonicalize(directory)
        .map_err(|error| FilesError::io("open directory", directory, error))?;
    if directory.is_dir() {
        Ok(directory)
    } else {
        Err(FilesError::InvalidPath {
            path: directory,
            message: "the location is not a directory",
        })
    }
}

fn read_directory(directory: &Path, sort: SortOrder) -> FilesResult<Vec<FileEntry>> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| FilesError::io("read directory", directory, error))?
        .map(|entry| {
            let entry =
                entry.map_err(|error| FilesError::io("read directory entry", directory, error))?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| FilesError::io("read metadata", &path, error))?;
            Ok(FileEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                path,
                kind: FileKind::from_file_type(metadata.file_type()),
                size: if metadata.is_file() {
                    metadata.len()
                } else {
                    0
                },
                modified: metadata.modified().ok(),
            })
        })
        .collect::<FilesResult<Vec<_>>>()?;
    sort_entries(&mut entries, sort);
    Ok(entries)
}

fn sort_entries(entries: &mut [FileEntry], sort: SortOrder) {
    entries.sort_by(|left, right| {
        let order = match sort.field {
            SortField::Name => left
                .name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase()),
            SortField::Kind => file_kind_rank(left.kind)
                .cmp(&file_kind_rank(right.kind))
                .then_with(|| left.name.cmp(&right.name)),
            SortField::Modified => right
                .modified
                .cmp(&left.modified)
                .then_with(|| left.name.cmp(&right.name)),
            SortField::Size => right
                .size
                .cmp(&left.size)
                .then_with(|| left.name.cmp(&right.name)),
        };
        if sort.descending {
            order.reverse()
        } else {
            order
        }
    });
}

const fn file_kind_rank(kind: FileKind) -> u8 {
    match kind {
        FileKind::Directory => 0,
        FileKind::File => 1,
        FileKind::Symlink => 2,
        FileKind::Other => 3,
    }
}

fn is_plain_name(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && Path::new(value).components().count() == 1
        && !Path::new(value).is_absolute()
}

fn destination_path(source: &Path, target: &Path) -> FilesResult<PathBuf> {
    let name = source.file_name().ok_or(FilesError::InvalidPath {
        path: source.to_path_buf(),
        message: "a filesystem root cannot be copied or moved",
    })?;
    let destination = target.join(name);
    if destination.exists() {
        return Err(FilesError::Conflict {
            path: destination,
            message: "target already contains an item with that name",
        });
    }
    Ok(destination)
}

fn ensure_not_recursive_directory(source: &Path, destination: &Path) -> FilesResult<()> {
    if source.is_dir() && destination.starts_with(source) {
        return Err(FilesError::Conflict {
            path: destination.to_path_buf(),
            message: "a directory cannot be copied or moved into itself",
        });
    }
    Ok(())
}

fn copy_recursively(source: &Path, destination: &Path) -> FilesResult<()> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| FilesError::io("read metadata", source, error))?;
    if metadata.file_type().is_dir() {
        fs::create_dir(destination)
            .map_err(|error| FilesError::io("create directory", destination, error))?;
        for child in
            fs::read_dir(source).map_err(|error| FilesError::io("read directory", source, error))?
        {
            let child =
                child.map_err(|error| FilesError::io("read directory entry", source, error))?;
            copy_recursively(&child.path(), &destination.join(child.file_name()))?;
        }
    } else if metadata.file_type().is_file() {
        fs::copy(source, destination)
            .map_err(|error| FilesError::io("copy file", source, error))?;
    } else {
        return Err(FilesError::InvalidPath {
            path: source.to_path_buf(),
            message: "copying links and special files requires a portal adapter",
        });
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let directory = std::env::args_os().nth(1).unwrap_or_else(|| ".".into());
    let trash = DirectoryTrash::new(std::env::temp_dir().join("sol-files-trash"));
    let mut files = FilesApp::new(directory, trash)?;
    files.app.start()?;
    println!("SOL Files — {}", files.active_tab().directory.display());
    println!(
        "{} entries; commands: {:?}",
        files.active_tab().entries.len(),
        FilesApp::<DirectoryTrash>::commands("")
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let nonce = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
            let root =
                std::env::temp_dir().join(format!("sol-files-test-{}-{nonce}", std::process::id()));
            fs::create_dir_all(&root).expect("fixture root should be created");
            Self { root }
        }

        fn path(&self, value: &str) -> PathBuf {
            self.root.join(value)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn app(fixture: &Fixture) -> FilesApp<DirectoryTrash> {
        FilesApp::new(&fixture.root, DirectoryTrash::new(fixture.path(".trash")))
            .expect("fixture directory should open")
    }

    fn write(fixture: &Fixture, name: &str, contents: &str) {
        fs::write(fixture.path(name), contents).expect("fixture file should be written");
    }

    #[test]
    fn directory_model_sorts_selects_navigates_and_projects_accessibility() {
        let fixture = Fixture::new();
        write(&fixture, "zebra.txt", "z");
        write(&fixture, "alpha.txt", "a");
        fs::create_dir(fixture.path("documents")).expect("fixture directory should be created");
        let mut files = app(&fixture);

        assert_eq!(
            files
                .active_tab()
                .entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            vec!["alpha.txt", "documents", "zebra.txt"]
        );
        files.set_layout(DirectoryLayout::Grid);
        files.set_sort(SortOrder {
            field: SortField::Kind,
            descending: false,
        });
        assert_eq!(files.active_tab().entries[0].kind, FileKind::Directory);
        assert_eq!(
            files
                .search("ALPHA")
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            vec!["alpha.txt"]
        );
        files.handle_key(FilesKey::ArrowDown).unwrap();
        files.handle_key(FilesKey::ShiftArrowDown).unwrap();
        assert_eq!(files.active_tab().selection.len(), 2);
        files.handle_key(FilesKey::SelectAll).unwrap();
        assert_eq!(files.active_tab().selection.len(), 3);
        files.select(fixture.path("documents")).unwrap();
        files.handle_key(FilesKey::Enter).unwrap();
        assert_eq!(
            files.active_tab().directory,
            fs::canonicalize(fixture.path("documents")).unwrap()
        );
        files.handle_key(FilesKey::Back).unwrap();
        assert_eq!(
            files.breadcrumbs().last(),
            Some(&fs::canonicalize(&fixture.root).unwrap())
        );
        assert_eq!(files.accessibility_tree().label, "Files");
        assert_eq!(files.chrome_tokens().1, Spacing::Md);
    }

    #[test]
    fn temp_fixture_round_trips_copy_move_rename_trash_restore_and_drop() {
        let fixture = Fixture::new();
        write(&fixture, "report.txt", "draft");
        fs::create_dir(fixture.path("inbox")).unwrap();
        fs::create_dir(fixture.path("archive")).unwrap();
        let mut files = app(&fixture);

        let copied = files
            .copy(fixture.path("report.txt"), fixture.path("inbox"))
            .unwrap();
        assert_eq!(fs::read_to_string(&copied).unwrap(), "draft");
        let moved = files.move_to(&copied, fixture.path("archive")).unwrap();
        assert!(!copied.exists());
        let renamed = files.rename(&moved, "final.txt").unwrap();
        assert!(renamed.ends_with("final.txt"));
        let trash = files.delete_to_trash(&renamed).unwrap();
        assert!(!renamed.exists());
        assert!(trash.trashed_path.exists());
        files.restore_from_trash(&trash).unwrap();
        assert_eq!(fs::read_to_string(&renamed).unwrap(), "draft");

        let dropped = files
            .perform_drop(DropRequest {
                sources: vec![fixture.path("report.txt")],
                target: fixture.path("inbox"),
                operation: DropOperation::Copy,
            })
            .unwrap();
        assert_eq!(dropped.len(), 1);
        assert_eq!(
            fs::read_to_string(fixture.path("inbox/report.txt")).unwrap(),
            "draft"
        );
    }

    #[test]
    fn commands_tabs_and_error_boundaries_are_deterministic() {
        let fixture = Fixture::new();
        write(&fixture, "visible.txt", "data");
        let mut files = app(&fixture);
        files.execute_command("view.grid").unwrap();
        files.execute_command("tab.new").unwrap();
        assert_eq!(files.tabs().len(), 2);
        files.close_tab(0).unwrap();
        assert_eq!(files.tabs().len(), 1);
        assert_eq!(
            FilesApp::<DirectoryTrash>::commands("refresh")[0].id,
            "directory.refresh"
        );
        assert!(matches!(
            files.rename(fixture.path("visible.txt"), "../escape"),
            Err(FilesError::InvalidPath { .. })
        ));
        assert_eq!(
            FilesError::io(
                "read",
                Path::new("forbidden"),
                io::Error::from(io::ErrorKind::PermissionDenied)
            )
            .kind(),
            FilesErrorKind::PermissionDenied
        );
        assert!(matches!(
            files.execute_command("missing"),
            Err(FilesError::UnknownCommand(_))
        ));
    }

    #[test]
    fn shared_palette_executes_files_commands_and_projects_empty_state() {
        let fixture = Fixture::new();
        write(&fixture, "visible.txt", "data");
        let mut files = app(&fixture);
        files
            .handle_command_palette_key(Key::CommandPalette)
            .unwrap();
        files.handle_command_palette_key(Key::Tab).unwrap();
        files.handle_command_palette_key(Key::Tab).unwrap();
        assert_eq!(
            files.handle_command_palette_key(Key::Space).unwrap(),
            CommandPaletteOutcome::Execute("view.grid")
        );
        assert_eq!(files.active_tab().layout, DirectoryLayout::Grid);
        files.handle_command_palette_key(Key::ShiftTab).unwrap();
        files.handle_command_palette_key(Key::ShiftTab).unwrap();
        files
            .handle_command_palette_key(Key::Character('z'))
            .unwrap();
        assert_eq!(
            files.command_palette_accessibility_tree().children[1].label,
            "No matching commands"
        );
        assert_eq!(
            files.handle_command_palette_key(Key::Escape).unwrap(),
            CommandPaletteOutcome::Closed
        );
    }
}
