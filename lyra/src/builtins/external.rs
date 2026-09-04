use crate::parser::Value;
use crate::runtime::{RuntimeError, RuntimeResult};
use std::collections::HashMap;
use std::process::ExitStatus;
use std::process::Stdio;
use tokio::process::{Child, Command};

#[derive(Debug, PartialEq, Eq)]
pub struct ExternalInvocation {
    pub command: String,
    pub has_environment_assignments: bool,
}

/// Inspect the beginning of a command line without changing its contents.
///
/// `shell_words` is deliberately only used for classification. The original
/// line is passed to `/bin/sh` so quotes, option order, expansions and shell
/// operators retain their normal meaning.
pub fn inspect_external_invocation(line: &str) -> RuntimeResult<Option<ExternalInvocation>> {
    let words = shell_words::split(line)
        .map_err(|error| RuntimeError::ParseError(format!("invalid command line: {error}")))?;

    let mut has_environment_assignments = false;
    for word in words {
        if is_environment_assignment(&word) {
            has_environment_assignments = true;
            continue;
        }

        return Ok(Some(ExternalInvocation {
            command: word,
            has_environment_assignments,
        }));
    }

    Ok(None)
}

fn is_environment_assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };

    let mut chars = name.chars();
    matches!(chars.next(), Some('_') | Some('a'..='z') | Some('A'..='Z'))
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

/// Execute a top-level external command exactly as entered by the user.
///
/// Going through the system shell provides the CLI compatibility users expect
/// from a shell: argument ordering and quoting, environment expansion, globs,
/// redirection, pipes and command chaining. All standard streams are inherited
/// so full-screen and interactive programs continue to own the terminal.
pub async fn execute_external_line(
    line: &str,
    environment: &HashMap<String, String>,
) -> RuntimeResult<Value> {
    let invocation = inspect_external_invocation(line)?.ok_or_else(|| {
        RuntimeError::ParseError("expected an external command to execute".to_string())
    })?;

    let mut child = Command::new("/bin/sh");
    child
        .arg("-c")
        .arg(line)
        .envs(environment)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    let mut child = child.spawn().map_err(RuntimeError::IoError)?;
    let status = wait_for_child(&mut child)
        .await
        .map_err(RuntimeError::IoError)?;

    if status.success() {
        return Ok(Value::Null);
    }

    // POSIX shells use 127 when command lookup fails. Surface Lyra's more
    // useful command-not-found diagnostic instead of a generic exit status.
    if status.code() == Some(127) {
        return Err(RuntimeError::UndefinedCommand(invocation.command));
    }

    Err(RuntimeError::CommandFailed {
        command: invocation.command,
        status: status.to_string(),
    })
}

pub async fn execute_external(
    name: &str,
    args: Vec<Value>,
    flags: HashMap<String, Value>,
) -> RuntimeResult<Value> {
    // 将 Value 参数转换为字符串
    let mut cmd_args = Vec::new();
    for arg in args {
        match arg {
            Value::String(s) => cmd_args.push(s),
            Value::Number(n) => cmd_args.push(n.to_string()),
            Value::Bool(b) => cmd_args.push(b.to_string()),
            _ => {
                return Err(RuntimeError::TypeError {
                    expected: "string, number, or bool".to_string(),
                    got: arg.type_name().to_string(),
                });
            }
        }
    }

    // This path is used by external calls nested inside Lyra expressions.
    // Top-level external commands use `execute_external_line`, which retains
    // exact option ordering. A stable order here is still preferable to
    // silently dropping every option, as the previous implementation did.
    let mut flags: Vec<_> = flags.into_iter().collect();
    flags.sort_by(|left, right| left.0.cmp(&right.0));
    for (name, value) in flags {
        let prefix = if name.len() == 1 { "-" } else { "--" };
        match value {
            Value::Bool(true) => cmd_args.push(format!("{prefix}{name}")),
            Value::Bool(false) => cmd_args.push(format!("{prefix}{name}=false")),
            Value::String(value) => cmd_args.push(format!("{prefix}{name}={value}")),
            Value::Number(value) => cmd_args.push(format!("{prefix}{name}={value}")),
            Value::Null => cmd_args.push(format!("{prefix}{name}")),
            other => {
                return Err(RuntimeError::TypeError {
                    expected: "string, number, bool, or null flag value".to_string(),
                    got: other.type_name().to_string(),
                });
            }
        }
    }

    // 执行外部命令
    let mut command = Command::new(name);
    command
        .args(&cmd_args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .stdin(Stdio::inherit())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            RuntimeError::UndefinedCommand(name.to_string())
        } else {
            RuntimeError::IoError(e)
        }
    })?;
    let status = wait_for_child(&mut child)
        .await
        .map_err(RuntimeError::IoError)?;

    if status.success() {
        Ok(Value::Null)
    } else {
        Err(RuntimeError::CommandFailed {
            command: name.to_string(),
            status: status.to_string(),
        })
    }
}

async fn wait_for_child(child: &mut Child) -> std::io::Result<ExitStatus> {
    #[cfg(unix)]
    loop {
        tokio::select! {
            status = child.wait() => return status,
            interrupt = tokio::signal::ctrl_c() => {
                interrupt?;

                // The terminal normally delivers SIGINT to the whole foreground
                // process group, including this child. Forwarding it explicitly
                // also covers callers that signal Lyra by PID. Tokio's signal
                // listener keeps Lyra itself alive so it can show another prompt.
                if let Some(id) = child.id() {
                    let _ = nix::sys::signal::kill(
                        nix::unistd::Pid::from_raw(id as i32),
                        nix::sys::signal::Signal::SIGINT,
                    );
                }
            }
        }
    }

    #[cfg(not(unix))]
    child.wait().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspects_regular_command_without_rebuilding_it() {
        let invocation = inspect_external_invocation("cargo test --package 'my crate'")
            .unwrap()
            .unwrap();

        assert_eq!(invocation.command, "cargo");
        assert!(!invocation.has_environment_assignments);
    }

    #[test]
    fn skips_leading_environment_assignments() {
        let invocation =
            inspect_external_invocation("RUST_LOG=debug CARGO_TERM_COLOR=always cargo test")
                .unwrap()
                .unwrap();

        assert_eq!(invocation.command, "cargo");
        assert!(invocation.has_environment_assignments);
    }

    #[test]
    fn rejects_an_unclosed_quote() {
        let error = inspect_external_invocation("printf 'unfinished").unwrap_err();
        assert!(matches!(error, RuntimeError::ParseError(_)));
    }

    #[tokio::test]
    async fn raw_external_line_preserves_options_quotes_and_redirection() {
        let path = std::env::temp_dir().join(format!(
            "lyra-external-{}-{}.txt",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let path_string = path.to_string_lossy();
        let quoted_path = shell_words::quote(&path_string);
        let line = format!("printf '%s' '--flag=a value' > {quoted_path}");

        execute_external_line(&line, &HashMap::new()).await.unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(path).unwrap();

        assert_eq!(contents, "--flag=a value");
    }
}
