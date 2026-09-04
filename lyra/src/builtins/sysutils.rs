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

    async fn execute_piped(
        &self,
        args: Vec<Value>,
        flags: HashMap<String, Value>,
        _input: Option<Value>,
        emit: bool,
    ) -> RuntimeResult<Value> {
        let mut vars: Vec<_> = std::env::vars().collect();
        vars.sort();
        let output = vars
            .into_iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("\n");
        let output = if output.is_empty() {
            output
        } else {
            format!("{output}\n")
        };
        if emit {
            print!("{output}");
        }
        let _ = (args, flags);
        Ok(Value::String(output))
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
        let result = basename_result(&args)?;
        println!("{result}");
        Ok(Value::String(result))
    }

    async fn execute_piped(
        &self,
        args: Vec<Value>,
        _flags: HashMap<String, Value>,
        _input: Option<Value>,
        emit: bool,
    ) -> RuntimeResult<Value> {
        let result = basename_result(&args)?;
        let output = format!("{result}\n");
        if emit {
            print!("{output}");
        }
        Ok(Value::String(output))
    }
}

fn basename_result(args: &[Value]) -> RuntimeResult<String> {
    let Some(Value::String(path_str)) = args.first() else {
        return match args.first() {
            None => Err(RuntimeError::Custom(
                "basename: missing operand".to_string(),
            )),
            Some(value) => Err(RuntimeError::TypeError {
                expected: "string".to_string(),
                got: value.type_name().to_string(),
            }),
        };
    };

    let path = PathBuf::from(path_str);
    let basename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_string();
    Ok(match args.get(1) {
        Some(Value::String(suffix)) => basename
            .strip_suffix(suffix)
            .unwrap_or(&basename)
            .to_string(),
        _ => basename,
    })
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
        let dirname = dirname_result(&args)?;
        println!("{dirname}");
        Ok(Value::String(dirname))
    }

    async fn execute_piped(
        &self,
        args: Vec<Value>,
        _flags: HashMap<String, Value>,
        _input: Option<Value>,
        emit: bool,
    ) -> RuntimeResult<Value> {
        let dirname = dirname_result(&args)?;
        let output = format!("{dirname}\n");
        if emit {
            print!("{output}");
        }
        Ok(Value::String(output))
    }
}

fn dirname_result(args: &[Value]) -> RuntimeResult<String> {
    let Some(Value::String(path_str)) = args.first() else {
        return match args.first() {
            None => Err(RuntimeError::Custom("dirname: missing operand".to_string())),
            Some(value) => Err(RuntimeError::TypeError {
                expected: "string".to_string(),
                got: value.type_name().to_string(),
            }),
        };
    };
    Ok(PathBuf::from(path_str)
        .parent()
        .and_then(|path| path.to_str())
        .unwrap_or(".")
        .to_string())
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
        args: Vec<Value>,
        flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        let output = date_output(&args, &flags);
        println!("{output}");
        Ok(Value::String(output))
    }

    async fn execute_piped(
        &self,
        args: Vec<Value>,
        flags: HashMap<String, Value>,
        _input: Option<Value>,
        emit: bool,
    ) -> RuntimeResult<Value> {
        let output = format!("{}\n", date_output(&args, &flags));
        if emit {
            print!("{output}");
        }
        Ok(Value::String(output))
    }
}

fn date_output(args: &[Value], flags: &HashMap<String, Value>) -> String {
    use chrono::Local;

    let format = flags
        .get("format")
        .or(flags.get("f"))
        .and_then(|value| match value {
            Value::String(value) => Some(value.as_str()),
            _ => None,
        })
        .or_else(|| {
            flags
                .get("format")
                .or(flags.get("f"))
                .filter(|value| matches!(value, Value::Bool(true)))
                .and_then(|_| args.first())
                .and_then(|value| match value {
                    Value::String(value) => Some(value.as_str()),
                    _ => None,
                })
        });
    let now = Local::now();
    format.map_or_else(
        || now.format("%a %b %d %H:%M:%S %Z %Y").to_string(),
        |value| now.format(value).to_string(),
    )
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
        Ok(Value::Bool(false))
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
        let username = effective_username()?;
        println!("{username}");
        Ok(Value::String(username))
    }

    async fn execute_piped(
        &self,
        _args: Vec<Value>,
        _flags: HashMap<String, Value>,
        _input: Option<Value>,
        emit: bool,
    ) -> RuntimeResult<Value> {
        let output = format!("{}\n", effective_username()?);
        if emit {
            print!("{output}");
        }
        Ok(Value::String(output))
    }
}

fn effective_username() -> RuntimeResult<String> {
    let uid = nix::unistd::geteuid();
    Ok(nix::unistd::User::from_uid(uid)
        .map_err(|error| RuntimeError::Custom(format!("whoami: {error}")))?
        .map_or_else(|| uid.as_raw().to_string(), |user| user.name))
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
        let output = uname_output(&flags);
        println!("{output}");
        Ok(Value::String(output))
    }

    async fn execute_piped(
        &self,
        _args: Vec<Value>,
        flags: HashMap<String, Value>,
        _input: Option<Value>,
        emit: bool,
    ) -> RuntimeResult<Value> {
        let output = format!("{}\n", uname_output(&flags));
        if emit {
            print!("{output}");
        }
        Ok(Value::String(output))
    }
}

fn uname_output(flags: &HashMap<String, Value>) -> String {
    let all = flags.get("a").or(flags.get("all")).is_some();
    let kernel_name = all || flags.get("s").or(flags.get("kernel-name")).is_some();
    let nodename = all || flags.get("n").or(flags.get("nodename")).is_some();
    let kernel_release = all || flags.get("r").or(flags.get("kernel-release")).is_some();
    let kernel_version = all || flags.get("v").or(flags.get("kernel-version")).is_some();
    let machine = all || flags.get("m").or(flags.get("machine")).is_some();
    let show_default =
        !all && !kernel_name && !nodename && !kernel_release && !kernel_version && !machine;
    let mut parts = Vec::new();

    if show_default || kernel_name {
        parts.push("Linux".to_string());
    }
    if nodename {
        parts.push(
            hostname::get()
                .ok()
                .and_then(|name| name.into_string().ok())
                .unwrap_or_else(|| "unknown".to_string()),
        );
    }
    if kernel_release {
        parts.push(read_kernel_field("/proc/sys/kernel/osrelease", "unknown"));
    }
    if kernel_version {
        parts.push(read_kernel_field("/proc/sys/kernel/version", "unknown"));
    }
    if machine {
        parts.push(std::env::consts::ARCH.to_string());
    }
    parts.join(" ")
}

fn read_kernel_field(path: &str, fallback: &str) -> String {
    std::fs::read_to_string(path)
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|_| fallback.to_string())
}
