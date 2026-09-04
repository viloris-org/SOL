use reedline::{Span, Suggestion};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::PathBuf;

/// Command name completer
/// Completes both built-in commands and executables in PATH
pub struct CommandCompleter {
    builtins: Vec<String>,
    path_commands: Vec<String>,
}

impl CommandCompleter {
    pub fn new() -> Self {
        let builtins = vec![
            // Basic commands
            "echo".to_string(),
            "ls".to_string(),
            "cd".to_string(),
            "pwd".to_string(),
            "exit".to_string(),
            "which".to_string(),
            "clear".to_string(),
            "reset".to_string(),
            // File operations
            "cat".to_string(),
            "cp".to_string(),
            "mv".to_string(),
            "rm".to_string(),
            "mkdir".to_string(),
            "touch".to_string(),
            // Text utilities
            "grep".to_string(),
            "head".to_string(),
            "tail".to_string(),
            "wc".to_string(),
            "sort".to_string(),
            "uniq".to_string(),
            // System utilities
            "env".to_string(),
            "basename".to_string(),
            "dirname".to_string(),
            "sleep".to_string(),
            "date".to_string(),
            "true".to_string(),
            "false".to_string(),
            "whoami".to_string(),
            "uname".to_string(),
            // Language constructs
            "let".to_string(),
            "if".to_string(),
            "for".to_string(),
            "while".to_string(),
        ];

        let path_commands = Self::discover_path_commands();

        Self {
            builtins,
            path_commands,
        }
    }

    pub fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        let before_cursor = &line[..pos];
        let partial = before_cursor.trim();

        // Find the start position of the command
        let start = before_cursor.len() - partial.len();
        let span = Span::new(start, pos);

        if partial.is_empty() {
            return Vec::new();
        }

        // Use simple prefix matching for better compatibility
        let mut scored_suggestions = Vec::new();

        // Match against built-ins (higher priority)
        for cmd in &self.builtins {
            if cmd.starts_with(partial) {
                scored_suggestions.push((
                    100,
                    Suggestion {
                        value: cmd.clone(),
                        description: Some("built-in".to_string()),
                        extra: None,
                        span,
                        append_whitespace: true,
                        style: None,
                    },
                ));
            }
        }

        // Match against PATH commands
        for cmd in &self.path_commands {
            if cmd.starts_with(partial) {
                scored_suggestions.push((
                    50,
                    Suggestion {
                        value: cmd.clone(),
                        description: Some("command".to_string()),
                        extra: None,
                        span,
                        append_whitespace: true,
                        style: None,
                    },
                ));
            }
        }

        // Sort by score (descending) and take top 20
        scored_suggestions.sort_by_key(|entry| std::cmp::Reverse(entry.0));
        scored_suggestions
            .into_iter()
            .take(20)
            .map(|(_, suggestion)| suggestion)
            .collect()
    }

    /// Discover all executable commands in PATH
    fn discover_path_commands() -> Vec<String> {
        let mut commands = HashSet::new();

        let path_var = env::var("PATH").unwrap_or_default();

        for dir_str in path_var.split(':') {
            let dir = PathBuf::from(dir_str);

            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    if let Ok(metadata) = entry.metadata() {
                        // Check if executable
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            let is_executable = metadata.permissions().mode() & 0o111 != 0;

                            if metadata.is_file()
                                && is_executable
                                && let Some(name) = entry.file_name().to_str()
                            {
                                commands.insert(name.to_string());
                            }
                        }

                        #[cfg(not(unix))]
                        {
                            if metadata.is_file() {
                                if let Some(name) = entry.file_name().to_str() {
                                    commands.insert(name.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut result: Vec<String> = commands.into_iter().collect();
        result.sort();
        result
    }
}

impl Default for CommandCompleter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_path_commands() {
        let commands = CommandCompleter::discover_path_commands();
        // Should find at least some common commands
        assert!(!commands.is_empty());
        // Most systems have 'ls'
        assert!(commands.contains(&"ls".to_string()));
    }

    #[test]
    fn test_builtin_commands() {
        let completer = CommandCompleter::new();
        // Basic commands
        assert!(completer.builtins.contains(&"echo".to_string()));
        assert!(completer.builtins.contains(&"ls".to_string()));
        assert!(completer.builtins.contains(&"cd".to_string()));
        // File operations
        assert!(completer.builtins.contains(&"cat".to_string()));
        assert!(completer.builtins.contains(&"cp".to_string()));
        assert!(completer.builtins.contains(&"mkdir".to_string()));
        // Text utilities
        assert!(completer.builtins.contains(&"grep".to_string()));
        assert!(completer.builtins.contains(&"sort".to_string()));
        // System utilities
        assert!(completer.builtins.contains(&"whoami".to_string()));
        assert!(completer.builtins.contains(&"date".to_string()));
    }
}
