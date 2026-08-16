//! Local, renderer-neutral previews for Files entries.
//!
//! The preview layer deliberately reads a bounded amount of local data and
//! never follows links. Native thumbnailers, remote locations, and portal
//! document previews can implement their own provider without changing the
//! Files surface contract.

use super::{FileEntry, FileKind, FilesError, FilesResult};
use image::{GenericImageView, ImageReader, Limits};
use std::fs;
use std::io::{BufReader, Read};
use std::path::PathBuf;

const PREVIEW_READ_LIMIT: u64 = 64 * 1024;
const TEXT_CHARACTER_LIMIT: usize = 2_048;
const THUMBNAIL_EDGE_LIMIT: u32 = 256;
const IMAGE_DIMENSION_LIMIT: u32 = 8_192;
const IMAGE_ALLOCATION_LIMIT: u64 = 64 * 1024 * 1024;

/// The content category selected by the local preview provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewKind {
    /// A directory is represented by metadata only.
    Directory,
    /// A bounded, valid UTF-8 text sample is available.
    Text,
    /// A bounded RGBA thumbnail was decoded from a supported image.
    Image,
    /// File contents are not safe or useful to render as text.
    Binary,
    /// Links are represented without following their target.
    Symlink,
    /// An unsupported filesystem object.
    Other,
}

/// Renderer-neutral image pixels bounded for the Files preview pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageThumbnail {
    /// Thumbnail width in logical pixels.
    pub width: u32,
    /// Thumbnail height in logical pixels.
    pub height: u32,
    /// Row-major RGBA8 pixels, exactly `width * height * 4` bytes.
    pub rgba: Vec<u8>,
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
    /// Bounded decoded pixels when [`PreviewKind::Image`] applies.
    pub thumbnail: Option<ImageThumbnail>,
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
        thumbnail: None,
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
        if let Some(thumbnail) = decode_thumbnail(entry) {
            return Ok(FilePreview {
                kind: PreviewKind::Image,
                thumbnail: Some(thumbnail),
                ..base
            });
        }
        return Ok(base);
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

fn decode_thumbnail(entry: &FileEntry) -> Option<ImageThumbnail> {
    let file = fs::File::open(&entry.path).ok()?;
    let mut reader = ImageReader::new(BufReader::new(file))
        .with_guessed_format()
        .ok()?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(IMAGE_DIMENSION_LIMIT);
    limits.max_image_height = Some(IMAGE_DIMENSION_LIMIT);
    limits.max_alloc = Some(IMAGE_ALLOCATION_LIMIT);
    reader.limits(limits);
    let image = reader.decode().ok()?;
    let (source_width, source_height) = image.dimensions();
    let pixels = if source_width > THUMBNAIL_EDGE_LIMIT || source_height > THUMBNAIL_EDGE_LIMIT {
        image
            .thumbnail(THUMBNAIL_EDGE_LIMIT, THUMBNAIL_EDGE_LIMIT)
            .to_rgba8()
    } else {
        image.to_rgba8()
    };
    let (width, height) = pixels.dimensions();
    Some(ImageThumbnail {
        width,
        height,
        rgba: pixels.into_raw(),
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
        image::RgbaImage::from_pixel(640, 320, image::Rgba([12, 34, 56, 255]))
            .save(fixture.root.join("image.png"))
            .unwrap();
        fs::write(fixture.root.join("bytes.bin"), [0_u8, 1, 2]).unwrap();
        fs::create_dir(fixture.root.join("folder")).unwrap();

        let text = local_preview(&fixture.entry("notes.txt")).unwrap();
        assert_eq!(text.kind, PreviewKind::Text);
        assert_eq!(text.text.as_deref(), Some("hello SOL"));
        assert!(!text.truncated);

        let image = local_preview(&fixture.entry("image.png")).unwrap();
        assert_eq!(image.kind, PreviewKind::Image);
        let thumbnail = image.thumbnail.expect("image should decode a thumbnail");
        assert_eq!((thumbnail.width, thumbnail.height), (256, 128));
        assert_eq!(
            thumbnail.rgba.len(),
            thumbnail.width as usize * thumbnail.height as usize * 4
        );
        assert_eq!(&thumbnail.rgba[..4], &[12, 34, 56, 255]);
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

    #[test]
    fn malformed_image_header_falls_back_to_binary() {
        let fixture = Fixture::new();
        fs::write(fixture.root.join("broken.png"), b"\x89PNG\r\n\x1a\nraw").unwrap();

        let preview = local_preview(&fixture.entry("broken.png")).unwrap();
        assert_eq!(preview.kind, PreviewKind::Binary);
        assert!(preview.thumbnail.is_none());
    }

    #[test]
    fn jpeg_gif_and_webp_decode_through_the_same_bounded_contract() {
        let fixture = Fixture::new();
        let image = image::RgbImage::from_pixel(32, 16, image::Rgb([80, 120, 160]));
        for name in ["image.jpg", "image.gif", "image.webp"] {
            image.save(fixture.root.join(name)).unwrap();
            let preview = local_preview(&fixture.entry(name)).unwrap();
            assert_eq!(preview.kind, PreviewKind::Image, "failed format: {name}");
            let thumbnail = preview.thumbnail.expect("supported image should decode");
            assert_eq!((thumbnail.width, thumbnail.height), (32, 16));
            assert_eq!(thumbnail.rgba.len(), 32 * 16 * 4);
        }
    }

    #[test]
    fn image_dimension_limit_rejects_oversized_decode() {
        let fixture = Fixture::new();
        image::RgbaImage::from_pixel(IMAGE_DIMENSION_LIMIT + 1, 1, image::Rgba([0, 0, 0, 255]))
            .save(fixture.root.join("too-wide.png"))
            .unwrap();

        let preview = local_preview(&fixture.entry("too-wide.png")).unwrap();
        assert_eq!(preview.kind, PreviewKind::Binary);
        assert!(preview.thumbnail.is_none());
    }
}
