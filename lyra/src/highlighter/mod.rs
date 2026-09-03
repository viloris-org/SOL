use nu_ansi_term::{Color, Style};
use reedline::{Highlighter, StyledText};

/// Syntax highlighter for Lyra Shell
/// Highlights commands, strings, variables, operators, etc.
pub struct LyraHighlighter {
    builtins: Vec<String>,
}

impl LyraHighlighter {
    pub fn new() -> Self {
        let builtins = vec![
            // Basic commands
            "echo", "ls", "cd", "pwd", "exit", "which", "clear", "reset",
            // File operations
            "cat", "cp", "mv", "rm", "mkdir", "touch", // Text utilities
            "grep", "head", "tail", "wc", "sort", "uniq", // System utilities
            "env", "basename", "dirname", "sleep", "date", "true", "false", "whoami", "uname",
            // Language constructs
            "let", "if", "else", "for", "while", "in",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        Self { builtins }
    }

    /// Apply syntax highlighting to input line
    fn highlight_line(&self, line: &str) -> StyledText {
        let mut styled = StyledText::new();
        let mut chars = line.chars().peekable();
        let mut current_token = String::new();
        let mut in_string = false;
        let mut string_quote = ' ';
        let mut is_first_token = true;

        while let Some(ch) = chars.next() {
            match ch {
                '"' | '\'' if !in_string => {
                    // Start of string
                    if !current_token.is_empty() {
                        self.push_token(&mut styled, &current_token, is_first_token);
                        current_token.clear();
                        is_first_token = false;
                    }
                    in_string = true;
                    string_quote = ch;
                    current_token.push(ch);
                }
                '"' | '\'' if in_string && ch == string_quote => {
                    // End of string
                    current_token.push(ch);
                    styled.push((Style::new().fg(Color::Green), current_token.clone()));
                    current_token.clear();
                    in_string = false;
                }
                '$' if !in_string => {
                    // Variable reference
                    if !current_token.is_empty() {
                        self.push_token(&mut styled, &current_token, is_first_token);
                        current_token.clear();
                        is_first_token = false;
                    }

                    // Collect variable name
                    let mut var_name = String::from("$");
                    while let Some(&next_ch) = chars.peek() {
                        if next_ch.is_alphanumeric() || next_ch == '_' {
                            var_name.push(next_ch);
                            chars.next();
                        } else {
                            break;
                        }
                    }

                    styled.push((Style::new().fg(Color::Cyan), var_name));
                }
                ' ' | '\t' if !in_string => {
                    // Whitespace - flush current token
                    if !current_token.is_empty() {
                        self.push_token(&mut styled, &current_token, is_first_token);
                        current_token.clear();
                        is_first_token = false;
                    }
                    styled.push((Style::default(), ch.to_string()));
                }
                '|' | '&' | ';' | '>' | '<' if !in_string => {
                    // Operators
                    if !current_token.is_empty() {
                        self.push_token(&mut styled, &current_token, is_first_token);
                        current_token.clear();
                        is_first_token = false;
                    }
                    styled.push((Style::new().fg(Color::Magenta), ch.to_string()));
                }
                _ => {
                    current_token.push(ch);
                }
            }
        }

        // Flush remaining token
        if !current_token.is_empty() {
            if in_string {
                // Unclosed string - highlight in red
                styled.push((Style::new().fg(Color::Red), current_token));
            } else {
                self.push_token(&mut styled, &current_token, is_first_token);
            }
        }

        styled
    }

    /// Push a token with appropriate styling
    fn push_token(&self, styled: &mut StyledText, token: &str, is_command_position: bool) {
        let style = if is_command_position {
            // First token: could be a builtin or command
            if self.builtins.contains(&token.to_string()) {
                Style::new().fg(Color::Blue) // Built-in command
            } else {
                Style::new().fg(Color::Yellow) // External command
            }
        } else if token.starts_with("--") || token.starts_with('-') {
            Style::new().fg(Color::Cyan) // Flag
        } else if token.parse::<f64>().is_ok() {
            Style::new().fg(Color::Magenta) // Number
        } else {
            Style::default() // Regular argument
        };

        styled.push((style, token.to_string()));
    }
}

impl Default for LyraHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl Highlighter for LyraHighlighter {
    fn highlight(&self, line: &str, _cursor: usize) -> StyledText {
        self.highlight_line(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_simple_command() {
        let highlighter = LyraHighlighter::new();
        let styled = highlighter.highlight_line("echo hello");
        assert!(!styled.buffer.is_empty());
    }

    #[test]
    fn test_highlight_builtin() {
        let highlighter = LyraHighlighter::new();
        assert!(highlighter.builtins.contains(&"echo".to_string()));
    }
}
