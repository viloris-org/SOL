use crate::builtins::Builtin;
use crate::parser::Value;
use crate::runtime::{RuntimeError, RuntimeResult};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;

/// env - display environment variables
pub struct Env;

#[async_trait]
impl Builtin for Env {
    fn name(&self) -> &str {
        "env"
    }

    fn description(&self) -> &str {
        "Display environment variables"
    }

    async fn execute(
        &self,
        _args: Vec<Value>,
        _flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        let mut vars: Vec<_> = std::env::vars().collect();
        vars.sort();

        for (key, value) in vars {
            println!("{}={}", key, value);
        }

        Ok(Value::Null)
    }
}

/// basename - strip directory from filenames
pub struct Basename;

#[async_trait]
impl Builtin for Basename {
    fn name(&self) -> &str {
        "basename"
    }

    fn description(&self) -> &str {
        "Strip directory from filename"
    }

    async fn execute(
        &self,
        args: Vec<Value>,
        _flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        if args.is_empty() {
            return Err(RuntimeError::Custom(
                "basename: missing operand".to_string(),
            ));
        }

        let path_str = match &args[0] {
            Value::String(s) => s,
            _ => {
                return Err(RuntimeError::TypeError {
                    expected: "string".to_string(),
                    got: args[0].type_name().to_string(),
                });
            }
        };

        let path = PathBuf::from(path_str);
        let basename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        // If second arg provided, remove that suffix
        let result = if args.len() > 1 {
            if let Value::String(suffix) = &args[1] {
                basename
                    .strip_suffix(suffix)
                    .unwrap_or(&basename)
                    .to_string()
            } else {
                basename
            }
        } else {
            basename
        };

        println!("{}", result);
        Ok(Value::String(result))
    }
}

/// dirname - strip last component from file name
pub struct Dirname;

#[async_trait]
impl Builtin for Dirname {
    fn name(&self) -> &str {
        "dirname"
    }

    fn description(&self) -> &str {
        "Strip last component from filename"
    }

    async fn execute(
        &self,
        args: Vec<Value>,
        _flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        if args.is_empty() {
            return Err(RuntimeError::Custom("dirname: missing operand".to_string()));
        }

        let path_str = match &args[0] {
            Value::String(s) => s,
            _ => {
                return Err(RuntimeError::TypeError {
                    expected: "string".to_string(),
                    got: args[0].type_name().to_string(),
                });
            }
        };

        let path = PathBuf::from(path_str);
        let dirname = path
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or(".")
            .to_string();

        println!("{}", dirname);
        Ok(Value::String(dirname))
    }
}

/// sleep - delay for a specified amount of time
pub struct Sleep;

#[async_trait]
impl Builtin for Sleep {
    fn name(&self) -> &str {
        "sleep"
    }

    fn description(&self) -> &str {
        "Delay for a specified amount of time"
    }

    async fn execute(
        &self,
        args: Vec<Value>,
        _flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        if args.is_empty() {
            return Err(RuntimeError::Custom("sleep: missing operand".to_string()));
        }

        let duration = match &args[0] {
            Value::Number(n) => *n,
            Value::String(s) => s.parse::<f64>().map_err(|_| {
                RuntimeError::Custom(format!("sleep: invalid time interval '{}'", s))
            })?,
            _ => {
                return Err(RuntimeError::TypeError {
                    expected: "number or string".to_string(),
                    got: args[0].type_name().to_string(),
                });
            }
        };

        if duration < 0.0 {
            return Err(RuntimeError::Custom(
                "sleep: invalid time interval".to_string(),
            ));
        }

        let duration_ms = (duration * 1000.0) as u64;
        tokio::time::sleep(tokio::time::Duration::from_millis(duration_ms)).await;

        Ok(Value::Null)
    }
}

/// date - display or set the system date and time
pub struct Date;

#[async_trait]
impl Builtin for Date {
    fn name(&self) -> &str {
        "date"
    }

    fn description(&self) -> &str {
        "Display the system date and time"
    }

    async fn execute(
        &self,
        _args: Vec<Value>,
        flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        use chrono::Local;

        let format = flags
            .get("format")
            .or(flags.get("f"))
            .and_then(|v| match v {
                Value::String(s) => Some(s.as_str()),
                _ => None,
            });

        let now = Local::now();

        let output = if let Some(fmt) = format {
            now.format(fmt).to_string()
        } else {
            // Default format: "Fri Aug 29 12:34:56 PDT 2026"
            now.format("%a %b %d %H:%M:%S %Z %Y").to_string()
        };

        println!("{}", output);
        Ok(Value::String(output))
    }
}

/// true - do nothing, successfully
pub struct True;

#[async_trait]
impl Builtin for True {
    fn name(&self) -> &str {
        "true"
    }

    fn description(&self) -> &str {
        "Return success (exit code 0)"
    }

    async fn execute(
        &self,
        _args: Vec<Value>,
        _flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        Ok(Value::Bool(true))
    }
}

/// false - do nothing, unsuccessfully
pub struct False;

#[async_trait]
impl Builtin for False {
    fn name(&self) -> &str {
        "false"
    }

    fn description(&self) -> &str {
        "Return failure (exit code 1)"
    }

    async fn execute(
        &self,
        _args: Vec<Value>,
        _flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        Err(RuntimeError::Custom("false".to_string()))
    }
}

/// whoami - print effective user name
pub struct Whoami;

#[async_trait]
impl Builtin for Whoami {
    fn name(&self) -> &str {
        "whoami"
    }

    fn description(&self) -> &str {
        "Print current username"
    }

    async fn execute(
        &self,
        _args: Vec<Value>,
        _flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        let username = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "unknown".to_string());

        println!("{}", username);
        Ok(Value::String(username))
    }
}

/// uname - print system information
pub struct Uname;

#[async_trait]
impl Builtin for Uname {
    fn name(&self) -> &str {
        "uname"
    }

    fn description(&self) -> &str {
        "Print system information"
    }

    async fn execute(
        &self,
        _args: Vec<Value>,
        flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        let all = flags.get("a").or(flags.get("all")).is_some();
        let kernel_name = all || flags.get("s").or(flags.get("kernel-name")).is_some();
        let nodename = all || flags.get("n").or(flags.get("nodename")).is_some();
        let kernel_release = all || flags.get("r").or(flags.get("kernel-release")).is_some();
        let kernel_version = all || flags.get("v").or(flags.get("kernel-version")).is_some();
        let machine = all || flags.get("m").or(flags.get("machine")).is_some();

        // Default: show kernel name only
        let show_default =
            !all && !kernel_name && !nodename && !kernel_release && !kernel_version && !machine;

        let mut parts = Vec::new();

        if show_default || kernel_name {
            parts.push(std::env::consts::OS.to_string());
        }

        if nodename {
            let hostname = hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| "unknown".to_string());
            parts.push(hostname);
        }

        if kernel_release {
            // Simplified: use OS version string
            parts.push("6.0.0".to_string());
        }

        if kernel_version {
            parts.push("#1 SMP".to_string());
        }

        if machine {
            parts.push(std::env::consts::ARCH.to_string());
        }

        let output = parts.join(" ");
        println!("{}", output);
        Ok(Value::String(output))
    }
}
