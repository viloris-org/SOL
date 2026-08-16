//! Local, renderer-neutral previews for Files entries.
//!
//! The preview layer deliberately reads a bounded amount of local data and
//! never follows links. Native thumbnailers, remote locations, and portal
//! document previews can implement their own provider without changing the
//! Files surface contract.

use super::{FileEntry, FileKind, FilesError, FilesResult};
use std::fs;
use std::io::Read;
use std::path::PathBuf;

const PREVIEW_READ_LIMIT: u64 = 64 * 1024;
const TEXT_CHARACTER_LIMIT: usize = 2_048;

/// The content category selected by the local preview provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewKind {
    /// A directory is represented by metadata only.
    Directory,
    /// A bounded, valid UTF-8 text sample is available.
    Text,
    /// A recognized image header is present; pixels are left to a renderer.
    Image,
    /// File contents are not safe or useful to render as text.
    Binary,
    /// Links are represented without following their target.
    Symlink,
    /// An unsupported filesystem object.
    Other,
}

/// Bounded, renderer-neutral data for an entry preview pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePreview {
    /// Path of the entry that was inspected.
    pub path: PathBuf,
    /// Content category chosen without invoking an external helper.
    pub kind: PreviewKind,
    /// Stable name shown by a future preview surface.
    pub name: String,
    /// File size known from the directory projection.
    pub size: u64,
    /// Bounded text content when [`PreviewKind::Text`] applies.
    pub text: Option<String>,
    /// Whether the displayed text was shortened to maintain the read budget.
    pub truncated: bool,
}

/// Read a bounded preview for one local Files entry.
///
/// This function never executes a file, invokes a thumbnailer, or follows a
/// symbolic link. It provides deterministic metadata/text data for SolUI while
/// preserving a narrow extension point for platform preview services.
pub fn local_preview(entry: &FileEntry) -> FilesResult<FilePreview> {
    let base = FilePreview {
        path: entry.path.clone(),
        kind: preview_kind(entry.kind),
        name: entry.name.clone(),
        size: entry.size,
        text: None,
        truncated: false,
    };
    if entry.kind != FileKind::File {
        return Ok(base);
    }

    let file = fs::File::open(&entry.path)
        .map_err(|source| FilesError::io("open preview", &entry.path, source))?;
    let mut bytes = Vec::new();
    file.take(PREVIEW_READ_LIMIT)
        .read_to_end(&mut bytes)
        .map_err(|source| FilesError::io("read preview", &entry.path, source))?;

    if image_header(&bytes) {
        return Ok(FilePreview {
            kind: PreviewKind::Image,
            ..base
        });
    }

    let Ok(text) = String::from_utf8(bytes) else {
        return Ok(base);
    };
    if text.contains('\0') {
        return Ok(base);
    }

    let truncated_by_read = entry.size > PREVIEW_READ_LIMIT;
    let mut displayed = String::new();
    let mut characters = text.chars();
    for _ in 0..TEXT_CHARACTER_LIMIT {
        let Some(character) = characters.next() else {
            break;
        };
        displayed.push(character);
    }
    let truncated = truncated_by_read || characters.next().is_some();
    Ok(FilePreview {
        kind: PreviewKind::Text,
        text: Some(displayed),
        truncated,
        ..base
    })
}

const fn preview_kind(kind: FileKind) -> PreviewKind {
    match kind {
        FileKind::Directory => PreviewKind::Directory,
        FileKind::File => PreviewKind::Binary,
        FileKind::Symlink => PreviewKind::Symlink,
        FileKind::Other => PreviewKind::Other,
    }
}

fn image_header(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(&[0xff, 0xd8, 0xff])
        || bytes.starts_with(b"GIF87a")
        || bytes.starts_with(b"GIF89a")
        || (bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"))
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
            let root = std::env::temp_dir().join(format!(
                "sol-files-preview-test-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir_all(&root).expect("preview fixture directory should be created");
            Self { root }
        }

        fn entry(&self, name: &str) -> FileEntry {
            let path = self.root.join(name);
            let metadata = fs::symlink_metadata(&path).expect("fixture entry should exist");
            FileEntry {
                name: name.to_owned(),
                path,
                kind: FileKind::from_file_type(metadata.file_type()),
                size: metadata.len(),
                modified: None,
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn text_image_binary_and_directory_previews_are_bounded_and_typed() {
        let fixture = Fixture::new();
        fs::write(fixture.root.join("notes.txt"), "hello SOL").unwrap();
        fs::write(fixture.root.join("image.png"), b"\x89PNG\r\n\x1a\nraw").unwrap();
        fs::write(fixture.root.join("bytes.bin"), [0_u8, 1, 2]).unwrap();
        fs::create_dir(fixture.root.join("folder")).unwrap();

        let text = local_preview(&fixture.entry("notes.txt")).unwrap();
        assert_eq!(text.kind, PreviewKind::Text);
        assert_eq!(text.text.as_deref(), Some("hello SOL"));
        assert!(!text.truncated);

        assert_eq!(
            local_preview(&fixture.entry("image.png")).unwrap().kind,
            PreviewKind::Image
        );
        assert_eq!(
            local_preview(&fixture.entry("bytes.bin")).unwrap().kind,
            PreviewKind::Binary
        );
        assert_eq!(
            local_preview(&fixture.entry("folder")).unwrap().kind,
            PreviewKind::Directory
        );
    }

    #[test]
    fn oversized_text_is_character_limited() {
        let fixture = Fixture::new();
        fs::write(
            fixture.root.join("large.txt"),
            "x".repeat(TEXT_CHARACTER_LIMIT + 1),
        )
        .unwrap();

        let preview = local_preview(&fixture.entry("large.txt")).unwrap();
        assert_eq!(preview.kind, PreviewKind::Text);
        assert_eq!(preview.text.unwrap().chars().count(), TEXT_CHARACTER_LIMIT);
        assert!(preview.truncated);
    }
}
