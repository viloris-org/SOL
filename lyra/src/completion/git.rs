use reedline::{Span, Suggestion};
use std::process::Command;

/// Git-aware completer
/// Provides completions for Git subcommands, branches, remotes, etc.
pub struct GitCompleter {
    git_subcommands: Vec<String>,
}

impl GitCompleter {
    pub fn new() -> Self {
        let git_subcommands = vec![
            "add", "branch", "checkout", "clone", "commit", "diff", "fetch", "init", "log",
            "merge", "pull", "push", "rebase", "reset", "status", "tag", "stash", "remote", "show",
            "rm", "mv", "bisect", "grep", "clean", "config",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        Self { git_subcommands }
    }

    pub fn complete(&self, line: &str, pos: usize) -> Vec<Suggestion> {
        let before_cursor = &line[..pos];
        let tokens: Vec<&str> = before_cursor.split_whitespace().collect();

        if tokens.len() < 2 {
            return Vec::new();
        }

        // tokens[0] is "git", tokens[1] is the subcommand or partial subcommand
        if tokens.len() == 2 {
            // Complete git subcommands
            return self.complete_subcommands(before_cursor, tokens[1], pos);
        }

        // tokens[1] is a subcommand, now complete context-specific items
        let subcommand = tokens[1];
        match subcommand {
            "checkout" | "branch" | "merge" | "rebase" | "switch" => {
                self.complete_branches(before_cursor, pos)
            }
            "remote" => self.complete_remotes(before_cursor, pos),
            _ => Vec::new(),
        }
    }

    /// Complete git subcommands
    fn complete_subcommands(&self, line: &str, partial: &str, pos: usize) -> Vec<Suggestion> {
        let start = line.rfind(partial).unwrap_or(pos);
        let span = Span::new(start, pos);

        self.git_subcommands
            .iter()
            .filter(|cmd| cmd.starts_with(partial))
            .map(|cmd| Suggestion {
                value: cmd.clone(),
                description: Some("git subcommand".to_string()),
                extra: None,
                span,
                append_whitespace: true,
                style: None,
            })
            .collect()
    }

    /// Complete git branch names
    fn complete_branches(&self, line: &str, pos: usize) -> Vec<Suggestion> {
        let branches = self.get_git_branches();

        let tokens: Vec<&str> = line.split_whitespace().collect();
        let partial = tokens.last().unwrap_or(&"");

        let start = line.rfind(partial).unwrap_or(pos);
        let span = Span::new(start, pos);

        branches
            .into_iter()
            .filter(|branch| branch.starts_with(partial))
            .map(|branch| Suggestion {
                value: branch.clone(),
                description: Some("branch".to_string()),
                extra: None,
                span,
                append_whitespace: true,
                style: None,
            })
            .collect()
    }

    /// Complete git remote names
    fn complete_remotes(&self, line: &str, pos: usize) -> Vec<Suggestion> {
        let remotes = self.get_git_remotes();

        let tokens: Vec<&str> = line.split_whitespace().collect();
        let partial = tokens.last().unwrap_or(&"");

        let start = line.rfind(partial).unwrap_or(pos);
        let span = Span::new(start, pos);

        remotes
            .into_iter()
            .filter(|remote| remote.starts_with(partial))
            .map(|remote| Suggestion {
                value: remote.clone(),
                description: Some("remote".to_string()),
                extra: None,
                span,
                append_whitespace: true,
                style: None,
            })
            .collect()
    }

    /// Get list of git branches in current repository
    fn get_git_branches(&self) -> Vec<String> {
        let output = Command::new("git")
            .args(["branch", "--format=%(refname:short)"])
            .output();

        match output {
            Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Get list of git remotes in current repository
    fn get_git_remotes(&self) -> Vec<String> {
        let output = Command::new("git").args(["remote"]).output();

        match output {
            Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            _ => Vec::new(),
        }
    }
}

impl Default for GitCompleter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_subcommands() {
        let completer = GitCompleter::new();
        assert!(completer.git_subcommands.contains(&"checkout".to_string()));
        assert!(completer.git_subcommands.contains(&"commit".to_string()));
        assert!(completer.git_subcommands.contains(&"branch".to_string()));
    }
}
