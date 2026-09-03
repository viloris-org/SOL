use anyhow::Result;
use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

/// History entry with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub command: String,
    pub timestamp: DateTime<Utc>,
    pub exit_status: Option<i32>,
    pub working_dir: String,
}

/// History manager for persistent command history
pub struct HistoryManager {
    history_file: PathBuf,
    entries: Vec<HistoryEntry>,
    max_entries: usize,
}

impl HistoryManager {
    pub fn new() -> Result<Self> {
        let history_file = Self::get_history_file()?;

        // Ensure parent directory exists
        if let Some(parent) = history_file.parent() {
            fs::create_dir_all(parent)?;
        }

        let entries = Self::load_history(&history_file)?;

        Ok(Self {
            history_file,
            entries,
            max_entries: 10_000,
        })
    }

    /// Add a command to history
    pub fn add(&mut self, command: String, exit_status: Option<i32>) -> Result<()> {
        let entry = HistoryEntry {
            command: command.clone(),
            timestamp: Utc::now(),
            exit_status,
            working_dir: std::env::current_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
        };

        self.entries.push(entry.clone());

        // Trim if exceeds max
        if self.entries.len() > self.max_entries {
            self.entries.drain(0..self.entries.len() - self.max_entries);
        }

        // Append to file
        self.append_to_file(&entry)?;

        Ok(())
    }

    /// Get all history entries
    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    /// Search history by pattern
    pub fn search(&self, pattern: &str) -> Vec<&HistoryEntry> {
        let pattern_lower = pattern.to_lowercase();
        self.entries
            .iter()
            .rev()
            .filter(|entry| entry.command.to_lowercase().contains(&pattern_lower))
            .collect()
    }

    /// Get recent N entries
    pub fn recent(&self, n: usize) -> Vec<&HistoryEntry> {
        self.entries.iter().rev().take(n).collect()
    }

    /// Clear all history
    pub fn clear(&mut self) -> Result<()> {
        self.entries.clear();
        fs::write(&self.history_file, "")?;
        Ok(())
    }

    /// Get history file path
    fn get_history_file() -> Result<PathBuf> {
        if let Some(proj_dirs) = ProjectDirs::from("org", "viloris", "lyra") {
            let data_dir = proj_dirs.data_dir();
            Ok(data_dir.join("history.jsonl"))
        } else {
            // Fallback to home directory
            let home = std::env::var("HOME").unwrap_or_else(|_| String::from("."));
            Ok(PathBuf::from(home).join(".lyra_history"))
        }
    }

    /// Load history from file
    fn load_history(path: &PathBuf) -> Result<Vec<HistoryEntry>> {
        if !path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut entries = Vec::new();

        for line in reader.lines() {
            if let Ok(line) = line {
                if let Ok(entry) = serde_json::from_str::<HistoryEntry>(&line) {
                    entries.push(entry);
                }
            }
        }

        Ok(entries)
    }

    /// Append entry to history file
    fn append_to_file(&self, entry: &HistoryEntry) -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.history_file)?;

        let json = serde_json::to_string(entry)?;
        writeln!(file, "{}", json)?;

        Ok(())
    }

    /// Save all history (useful after trimming)
    pub fn save(&self) -> Result<()> {
        let mut file = File::create(&self.history_file)?;

        for entry in &self.entries {
            let json = serde_json::to_string(entry)?;
            writeln!(file, "{}", json)?;
        }

        Ok(())
    }
}

impl Default for HistoryManager {
    fn default() -> Self {
        Self::new().expect("Failed to initialize history manager")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_entry_creation() {
        let entry = HistoryEntry {
            command: "echo test".to_string(),
            timestamp: Utc::now(),
            exit_status: Some(0),
            working_dir: "/tmp".to_string(),
        };

        assert_eq!(entry.command, "echo test");
        assert_eq!(entry.exit_status, Some(0));
    }

    #[test]
    fn test_history_search() {
        let manager = HistoryManager {
            history_file: PathBuf::from("/tmp/test_history"),
            entries: vec![
                HistoryEntry {
                    command: "git status".to_string(),
                    timestamp: Utc::now(),
                    exit_status: Some(0),
                    working_dir: "/tmp".to_string(),
                },
                HistoryEntry {
                    command: "cargo build".to_string(),
                    timestamp: Utc::now(),
                    exit_status: Some(0),
                    working_dir: "/tmp".to_string(),
                },
                HistoryEntry {
                    command: "git commit".to_string(),
                    timestamp: Utc::now(),
                    exit_status: Some(0),
                    working_dir: "/tmp".to_string(),
                },
            ],
            max_entries: 10_000,
        };

        let results = manager.search("git");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_recent_entries() {
        let manager = HistoryManager {
            history_file: PathBuf::from("/tmp/test_history"),
            entries: vec![
                HistoryEntry {
                    command: "cmd1".to_string(),
                    timestamp: Utc::now(),
                    exit_status: Some(0),
                    working_dir: "/tmp".to_string(),
                },
                HistoryEntry {
                    command: "cmd2".to_string(),
                    timestamp: Utc::now(),
                    exit_status: Some(0),
                    working_dir: "/tmp".to_string(),
                },
                HistoryEntry {
                    command: "cmd3".to_string(),
                    timestamp: Utc::now(),
                    exit_status: Some(0),
                    working_dir: "/tmp".to_string(),
                },
            ],
            max_entries: 10_000,
        };

        let recent = manager.recent(2);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].command, "cmd3");
        assert_eq!(recent[1].command, "cmd2");
    }
}
