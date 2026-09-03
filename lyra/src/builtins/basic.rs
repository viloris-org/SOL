use crate::builtins::Builtin;
use crate::parser::Value;
use crate::runtime::{RuntimeError, RuntimeResult};
use async_trait::async_trait;
use std::collections::HashMap;

pub struct Echo;

#[async_trait]
impl Builtin for Echo {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> &str {
        "Print arguments to stdout"
    }

    async fn execute(
        &self,
        args: Vec<Value>,
        _flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        let output = args
            .iter()
            .map(|v| match v {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                Value::Null => "null".to_string(),
                _ => format!("{:?}", v),
            })
            .collect::<Vec<_>>()
            .join(" ");

        println!("{}", output);
        Ok(Value::Null)
    }
}

pub struct Pwd;

#[async_trait]
impl Builtin for Pwd {
    fn name(&self) -> &str {
        "pwd"
    }

    fn description(&self) -> &str {
        "Print current working directory"
    }

    async fn execute(
        &self,
        _args: Vec<Value>,
        _flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        let cwd = std::env::current_dir()?;
        let path = cwd.to_string_lossy().to_string();
        println!("{}", path);
        Ok(Value::String(path))
    }
}

pub struct Cd;

#[async_trait]
impl Builtin for Cd {
    fn name(&self) -> &str {
        "cd"
    }

    fn description(&self) -> &str {
        "Change current directory"
    }

    async fn execute(
        &self,
        args: Vec<Value>,
        _flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        let path = if args.is_empty() {
            // 没有参数，cd 到 home
            std::env::var("HOME").unwrap_or_else(|_| "/".to_string())
        } else {
            match &args[0] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(RuntimeError::TypeError {
                        expected: "string".to_string(),
                        got: args[0].type_name().to_string(),
                    });
                }
            }
        };

        std::env::set_current_dir(&path)?;
        Ok(Value::Null)
    }
}

pub struct Exit;

#[async_trait]
impl Builtin for Exit {
    fn name(&self) -> &str {
        "exit"
    }

    fn description(&self) -> &str {
        "Exit the shell"
    }

    async fn execute(
        &self,
        args: Vec<Value>,
        _flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        let code = if args.is_empty() {
            0
        } else {
            match &args[0] {
                Value::Number(n) => *n as i32,
                _ => 0,
            }
        };

        std::process::exit(code);
    }
}

pub struct Ls;

#[async_trait]
impl Builtin for Ls {
    fn name(&self) -> &str {
        "ls"
    }

    fn description(&self) -> &str {
        "List directory contents"
    }

    async fn execute(
        &self,
        args: Vec<Value>,
        flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        let path = if args.is_empty() {
            std::env::current_dir()?
        } else {
            match &args[0] {
                Value::String(s) => std::path::PathBuf::from(s),
                _ => std::env::current_dir()?,
            }
        };

        let show_hidden = flags
            .get("all")
            .or_else(|| flags.get("a"))
            .and_then(|v| match v {
                Value::Bool(b) => Some(*b),
                _ => None,
            })
            .unwrap_or(false);

        let long_format = flags
            .get("long")
            .or_else(|| flags.get("l"))
            .and_then(|v| match v {
                Value::Bool(b) => Some(*b),
                _ => None,
            })
            .unwrap_or(false);

        if long_format {
            // Collect entries for table format
            let mut entries = Vec::new();

            for entry in std::fs::read_dir(&path)? {
                let entry = entry?;
                let file_name = entry.file_name();
                let name = file_name.to_string_lossy().to_string();

                if !show_hidden && name.starts_with('.') {
                    continue;
                }

                let metadata = entry.metadata()?;
                let is_dir = metadata.is_dir();
                let is_symlink = metadata.is_symlink();
                let size = metadata.len();

                let file_type = if is_dir {
                    "dir"
                } else if is_symlink {
                    "link"
                } else {
                    "file"
                };

                let mut row = HashMap::new();
                row.insert("name".to_string(), Value::String(name));
                row.insert("type".to_string(), Value::String(file_type.to_string()));
                row.insert("size".to_string(), Value::Number(size as f64));

                entries.push(row);
            }

            let columns = vec!["name".to_string(), "type".to_string(), "size".to_string()];
            Ok(Value::Table {
                columns,
                rows: entries,
            })
        } else {
            // Collect entries for grid format
            let mut entries: Vec<(String, bool, bool)> = Vec::new();

            for entry in std::fs::read_dir(&path)? {
                let entry = entry?;
                let file_name = entry.file_name();
                let name = file_name.to_string_lossy().to_string();

                if !show_hidden && name.starts_with('.') {
                    continue;
                }

                let metadata = entry.metadata()?;
                let is_dir = metadata.is_dir();
                let is_symlink = metadata.is_symlink();

                entries.push((name, is_dir, is_symlink));
            }

            // Sort alphabetically (case-insensitive)
            entries.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

            // Get terminal width (fallback to 80 if detection fails)
            let term_width = term_size::dimensions().map(|(w, _)| w).unwrap_or(80);

            // Find max item width
            let max_width = entries
                .iter()
                .map(|(name, _, _)| name.len())
                .max()
                .unwrap_or(0);
            let col_width = (max_width + 3).min(term_width / 2); // Add padding
            let num_cols = (term_width / col_width).max(1);

            // Print items in columns
            for (i, (name, is_dir, is_symlink)) in entries.iter().enumerate() {
                if *is_symlink {
                    print!("\x1b[36m{:<width$}\x1b[0m", name, width = col_width); // Cyan for symlinks
                } else if *is_dir {
                    print!("\x1b[34m{:<width$}\x1b[0m", name, width = col_width); // Blue for directories
                } else {
                    print!("{:<width$}", name, width = col_width); // Default for files
                }

                if (i + 1) % num_cols == 0 {
                    println!();
                }
            }

            // Add final newline if needed
            if !entries.is_empty() && entries.len() % num_cols != 0 {
                println!();
            }

            Ok(Value::Null)
        }
    }
}

pub struct Which;

#[async_trait]
impl Builtin for Which {
    fn name(&self) -> &str {
        "which"
    }

    fn description(&self) -> &str {
        "Locate a command"
    }

    async fn execute(
        &self,
        args: Vec<Value>,
        _flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        if args.is_empty() {
            eprintln!("which: missing command name");
            return Ok(Value::Null);
        }

        let command_name = match &args[0] {
            Value::String(s) => s.clone(),
            _ => {
                return Err(RuntimeError::TypeError {
                    expected: "string".to_string(),
                    got: args[0].type_name().to_string(),
                });
            }
        };

        // Check if it's a builtin command
        let builtins = vec![
            "echo", "pwd", "cd", "exit", "ls", "which", "clear", "reset", "cat", "cp", "mv", "rm",
            "mkdir", "touch", "grep", "head", "tail", "wc", "sort", "uniq", "env", "basename",
            "dirname", "sleep", "date", "true", "false", "whoami", "uname",
        ];
        if builtins.contains(&command_name.as_str()) {
            println!("{}: shell built-in command", command_name);
            return Ok(Value::String(format!("{}: built-in", command_name)));
        }

        // Search in PATH
        if let Ok(path_var) = std::env::var("PATH") {
            for dir in path_var.split(':') {
                let full_path = std::path::Path::new(dir).join(&command_name);
                if full_path.exists() && full_path.is_file() {
                    let path_str = full_path.to_string_lossy().to_string();
                    println!("{}", path_str);
                    return Ok(Value::String(path_str));
                }
            }
        }

        eprintln!("{} not found", command_name);
        Ok(Value::Null)
    }
}

pub struct Clear;

#[async_trait]
impl Builtin for Clear {
    fn name(&self) -> &str {
        "clear"
    }

    fn description(&self) -> &str {
        "Clear the terminal screen"
    }

    async fn execute(
        &self,
        _args: Vec<Value>,
        _flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        // ANSI escape sequence to clear screen and move cursor to top-left
        print!("\x1B[2J\x1B[1;1H");
        use std::io::Write;
        std::io::stdout().flush()?;
        Ok(Value::Null)
    }
}

pub struct Reset;

#[async_trait]
impl Builtin for Reset {
    fn name(&self) -> &str {
        "reset"
    }

    fn description(&self) -> &str {
        "Reset the terminal to initial state"
    }

    async fn execute(
        &self,
        _args: Vec<Value>,
        _flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        // Full terminal reset sequence
        // ESC c - Reset terminal to initial state
        print!("\x1Bc");
        use std::io::Write;
        std::io::stdout().flush()?;
        Ok(Value::Null)
    }
}

pub struct Help;

#[async_trait]
impl Builtin for Help {
    fn name(&self) -> &str {
        "help"
    }

    fn description(&self) -> &str {
        "Display help information about builtin commands"
    }

    async fn execute(
        &self,
        args: Vec<Value>,
        _flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        if args.is_empty() {
            // Show all commands
            println!("Lyra Shell - Available Commands\n");

            // Get all commands from registry
            let registry = crate::builtins::BuiltinRegistry::new();
            let mut commands = registry.all_commands();
            commands.sort_by(|a, b| a.0.cmp(b.0));

            // Find max command name length for alignment
            let max_len = commands
                .iter()
                .map(|(name, _)| name.len())
                .max()
                .unwrap_or(0);

            // Print categories
            println!("\x1b[1mBasic Commands:\x1b[0m");
            for (name, desc) in &commands {
                if [
                    "echo", "pwd", "cd", "exit", "ls", "which", "clear", "reset", "help",
                ]
                .contains(name)
                {
                    println!(
                        "  \x1b[36m{:<width$}\x1b[0m  {}",
                        name,
                        desc,
                        width = max_len
                    );
                }
            }

            println!("\n\x1b[1mFile Operations:\x1b[0m");
            for (name, desc) in &commands {
                if ["cat", "cp", "mv", "rm", "mkdir", "touch"].contains(name) {
                    println!(
                        "  \x1b[36m{:<width$}\x1b[0m  {}",
                        name,
                        desc,
                        width = max_len
                    );
                }
            }

            println!("\n\x1b[1mText Utilities:\x1b[0m");
            for (name, desc) in &commands {
                if ["grep", "head", "tail", "wc", "sort", "uniq"].contains(name) {
                    println!(
                        "  \x1b[36m{:<width$}\x1b[0m  {}",
                        name,
                        desc,
                        width = max_len
                    );
                }
            }

            println!("\n\x1b[1mSystem Utilities:\x1b[0m");
            for (name, desc) in &commands {
                if [
                    "env", "basename", "dirname", "sleep", "date", "true", "false", "whoami",
                    "uname",
                ]
                .contains(name)
                {
                    println!(
                        "  \x1b[36m{:<width$}\x1b[0m  {}",
                        name,
                        desc,
                        width = max_len
                    );
                }
            }

            println!("\nType 'help <command>' for more information on a specific command.");
        } else {
            // Show help for specific command
            let cmd_name = match &args[0] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(RuntimeError::TypeError {
                        expected: "string".to_string(),
                        got: args[0].type_name().to_string(),
                    });
                }
            };

            let registry = crate::builtins::BuiltinRegistry::new();
            if let Some(cmd) = registry.get_command(&cmd_name) {
                println!("\x1b[1m{}\x1b[0m - {}", cmd.name(), cmd.description());
            } else {
                eprintln!(
                    "help: no help available for '{}', command not found",
                    cmd_name
                );
            }
        }

        Ok(Value::Null)
    }
}
