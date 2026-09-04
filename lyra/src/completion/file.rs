use reedline::{Span, Suggestion};
use std::fs;
use std::path::{Path, PathBuf};

/// File and directory path completer
pub struct FileCompleter;

impl FileCompleter {
    pub fn new() -> Self {
        Self
    }

    pub fn complete(&self, line: &str, pos: usize) -> Vec<Suggestion> {
        let before_cursor = &line[..pos];

        // Find the last token (the partial path being completed)
        let tokens: Vec<&str> = before_cursor.split_whitespace().collect();
        let partial = if before_cursor.ends_with(char::is_whitespace) {
            ""
        } else {
            tokens.last().copied().unwrap_or("")
        };

        // Determine the directory to search and the prefix to match
        let (search_dir, prefix, display_prefix) = self.parse_partial_path(partial);

        // Get the span for replacement - start from where the partial begins
        let start = pos - partial.len();
        let span = Span::new(start, pos);

        // List directory entries and filter
        self.list_completions(&search_dir, &prefix, &display_prefix, span)
    }

    /// Parse a partial path into (directory to search, filename prefix)
    fn parse_partial_path(&self, partial: &str) -> (PathBuf, String, String) {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        if partial.is_empty() {
            return (current_dir, String::new(), String::new());
        }

        let path = if partial.starts_with('/') {
            PathBuf::from(partial)
        } else if let Some(rest) = partial.strip_prefix("~/") {
            let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/"));
            PathBuf::from(home).join(rest)
        } else {
            current_dir.join(partial)
        };

        if partial.ends_with('/') {
            (path, String::new(), partial.to_string())
        } else {
            let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
            let prefix = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let display_prefix = partial
                .rfind('/')
                .map_or_else(String::new, |index| partial[..=index].to_string());
            (dir, prefix, display_prefix)
        }
    }

    /// List all matching files/directories
    fn list_completions(
        &self,
        dir: &Path,
        prefix: &str,
        display_prefix: &str,
        span: Span,
    ) -> Vec<Suggestion> {
        let Ok(entries) = fs::read_dir(dir) else {
            return Vec::new();
        };

        let mut suggestions = Vec::new();

        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();

            // Skip hidden files unless prefix starts with '.'
            if name.starts_with('.') && !prefix.starts_with('.') {
                continue;
            }

            // Filter by prefix
            if !name.starts_with(prefix) {
                continue;
            }

            let mut value = format!("{display_prefix}{name}");
            let mut description = None;

            // Add trailing slash for directories
            if entry.path().is_dir() {
                value.push('/');
                description = Some("directory".to_string());
            } else if let Ok(metadata) = entry.metadata() {
                let size = metadata.len();
                description = Some(format_file_size(size));
            }

            suggestions.push(Suggestion {
                value,
                description,
                extra: None,
                span,
                append_whitespace: false,
                style: None,
            });
        }

        // Sort: directories first, then alphabetically
        suggestions.sort_by(|a, b| {
            let a_is_dir = a.value.ends_with('/');
            let b_is_dir = b.value.ends_with('/');

            match (a_is_dir, b_is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.value.cmp(&b.value),
            }
        });

        suggestions
    }
}

impl Default for FileCompleter {
    fn default() -> Self {
        Self::new()
    }
}

/// Format file size in human-readable format
fn format_file_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = size as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    if unit_idx == 0 {
        format!("{} {}", size as u64, UNITS[unit_idx])
    } else {
        format!("{:.1} {}", size, UNITS[unit_idx])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_file_size() {
        assert_eq!(format_file_size(512), "512 B");
        assert_eq!(format_file_size(1536), "1.5 KB");
        assert_eq!(format_file_size(1_048_576), "1.0 MB");
        assert_eq!(format_file_size(1_073_741_824), "1.0 GB");
    }

    #[test]
    fn test_parse_partial_path_empty() {
        let completer = FileCompleter::new();
        let (dir, prefix, display_prefix) = completer.parse_partial_path("");
        assert_eq!(prefix, "");
        assert_eq!(display_prefix, "");
        assert!(dir.is_absolute() || dir == Path::new("."));
    }

    #[test]
    fn test_parse_partial_path_relative() {
        let completer = FileCompleter::new();
        let (dir, prefix, display_prefix) = completer.parse_partial_path("src/ma");
        assert_eq!(prefix, "ma");
        assert_eq!(display_prefix, "src/");
        assert!(dir.ends_with("src"));
    }
}
