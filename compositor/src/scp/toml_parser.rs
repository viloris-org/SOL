//! Minimal TOML parser for SOL manifests.
//!
//! This is a basic TOML parser that handles the subset needed for app manifests.
//! It supports basic key-value pairs, tables, and nested structures.

use std::collections::HashMap;

#[derive(Debug)]
pub enum TomlValue {
    String(String),
    Boolean(bool),
    Integer(i64),
    Table(HashMap<String, TomlValue>),
}

impl TomlValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            TomlValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            TomlValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_table(&self) -> Option<&HashMap<String, TomlValue>> {
        match self {
            TomlValue::Table(t) => Some(t),
            _ => None,
        }
    }
}

pub type TomlTable = HashMap<String, TomlValue>;

/// Parse a minimal TOML document.
///
/// Supports:
/// - Basic key = "value" pairs
/// - Boolean values (true/false)
/// - Integer values
/// - [section] headers
/// - [section.subsection] nested headers
pub fn parse(input: &str) -> Result<TomlTable, String> {
    let mut root = TomlTable::new();
    let mut current_table = &mut root;

    for (line_num, line) in input.lines().enumerate() {
        let trimmed = line.trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Handle [section] headers
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let section = &trimmed[1..trimmed.len() - 1];
            let table_path: Vec<String> = section.split('.').map(|s| s.to_string()).collect();

            // Navigate/create nested table structure
            current_table = &mut root;
            for part in &table_path {
                current_table = current_table
                    .entry(part.clone())
                    .or_insert_with(|| TomlValue::Table(HashMap::new()))
                    .as_table_mut()
                    .ok_or_else(|| format!("Line {}: {} is not a table", line_num + 1, part))?;
            }
            continue;
        }

        // Handle key = value pairs
        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim();
            let value = value.trim();

            let parsed_value = parse_value(value)
                .map_err(|e| format!("Line {}: {}", line_num + 1, e))?;

            current_table.insert(key.to_string(), parsed_value);
        } else {
            return Err(format!("Line {}: invalid syntax", line_num + 1));
        }
    }

    Ok(root)
}

fn parse_value(value: &str) -> Result<TomlValue, String> {
    let value = value.trim();

    // String (quoted)
    if (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
    {
        return Ok(TomlValue::String(value[1..value.len() - 1].to_string()));
    }

    // Boolean
    if value == "true" {
        return Ok(TomlValue::Boolean(true));
    }
    if value == "false" {
        return Ok(TomlValue::Boolean(false));
    }

    // Integer
    if let Ok(i) = value.parse::<i64>() {
        return Ok(TomlValue::Integer(i));
    }

    Err(format!("unable to parse value: {}", value))
}

impl TomlValue {
    fn as_table_mut(&mut self) -> Option<&mut HashMap<String, TomlValue>> {
        match self {
            TomlValue::Table(t) => Some(t),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_toml() {
        let input = r#"
# Comment
name = "test-app"
version = "1.0.0"
enabled = true

[capabilities]
clipboard = true
audio = false
        "#;

        let parsed = parse(input).expect("parse");
        assert_eq!(parsed.get("name").and_then(|v| v.as_str()), Some("test-app"));
        assert_eq!(parsed.get("version").and_then(|v| v.as_str()), Some("1.0.0"));
        assert_eq!(parsed.get("enabled").and_then(|v| v.as_bool()), Some(true));

        let caps = parsed.get("capabilities").and_then(|v| v.as_table()).expect("capabilities table");
        assert_eq!(caps.get("clipboard").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(caps.get("audio").and_then(|v| v.as_bool()), Some(false));
    }

    #[test]
    fn parses_nested_sections() {
        let input = r#"
[app]
id = "com.example.test"

[capabilities.static_caps]
clipboard = true
        "#;

        let parsed = parse(input).expect("parse");
        let app = parsed.get("app").and_then(|v| v.as_table()).expect("app table");
        assert_eq!(app.get("id").and_then(|v| v.as_str()), Some("com.example.test"));

        let caps = parsed.get("capabilities").and_then(|v| v.as_table()).expect("capabilities");
        let static_caps = caps.get("static_caps").and_then(|v| v.as_table()).expect("static_caps");
        assert_eq!(static_caps.get("clipboard").and_then(|v| v.as_bool()), Some(true));
    }
}
