use reedline::{Completer, Suggestion};

use super::command::CommandCompleter;
use super::file::FileCompleter;
use super::git::GitCompleter;

/// Main completion engine for Lyra Shell
/// Combines multiple completion strategies based on context
pub struct LyraCompleter {
    file_completer: FileCompleter,
    command_completer: CommandCompleter,
    git_completer: GitCompleter,
}

impl LyraCompleter {
    pub fn new() -> Self {
        Self {
            file_completer: FileCompleter::new(),
            command_completer: CommandCompleter::new(),
            git_completer: GitCompleter::new(),
        }
    }

    /// Determine the context and route to appropriate completer
    fn get_completion_context(&self, line: &str, pos: usize) -> CompletionContext {
        let before_cursor = &line[..pos];
        let tokens: Vec<&str> = before_cursor.split_whitespace().collect();

        if tokens.is_empty() {
            return CompletionContext::Command;
        }

        let first_token = tokens[0];

        // Git command context
        if first_token == "git"
            && (tokens.len() > 1 || before_cursor.ends_with(char::is_whitespace))
        {
            return CompletionContext::Git;
        }

        // After the first token, complete files/paths
        if tokens.len() > 1 || before_cursor.ends_with(char::is_whitespace) {
            return CompletionContext::Path;
        }

        // First position: complete commands
        CompletionContext::Command
    }
}

impl Default for LyraCompleter {
    fn default() -> Self {
        Self::new()
    }
}

impl Completer for LyraCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        let context = self.get_completion_context(line, pos);

        match context {
            CompletionContext::Command => self.command_completer.complete(line, pos),
            CompletionContext::Path => self.file_completer.complete(line, pos),
            CompletionContext::Git => self.git_completer.complete(line, pos),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompletionContext {
    Command,
    Path,
    Git,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completion_context_command() {
        let completer = LyraCompleter::new();
        let ctx = completer.get_completion_context("ec", 2);
        assert_eq!(ctx, CompletionContext::Command);
    }

    #[test]
    fn test_completion_context_path() {
        let completer = LyraCompleter::new();
        let ctx = completer.get_completion_context("ls /ho", 6);
        assert_eq!(ctx, CompletionContext::Path);
    }

    #[test]
    fn test_completion_context_git() {
        let completer = LyraCompleter::new();
        let ctx = completer.get_completion_context("git che", 7);
        assert_eq!(ctx, CompletionContext::Git);
    }
}
