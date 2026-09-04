use crate::parser::Value;
use crate::runtime::{RuntimeError, RuntimeResult};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

pub async fn execute_external(name: &str, args: Vec<Value>) -> RuntimeResult<Value> {
    let cmd_args = values_to_args(args)?;

    let status = Command::new(name)
        .args(&cmd_args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .stdin(Stdio::inherit())
        .status()
        .await
        .map_err(|error| map_spawn_error(name, error))?;

    if status.success() {
        Ok(Value::Bool(true))
    } else {
        Ok(Value::Bool(false))
    }
}

pub async fn execute_external_piped(
    name: &str,
    args: Vec<Value>,
    input: Option<Value>,
    emit: bool,
) -> RuntimeResult<Value> {
    let cmd_args = values_to_args(args)?;
    let mut command = Command::new(name);
    command
        .args(&cmd_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    if input.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::inherit());
    }

    let mut child = command
        .spawn()
        .map_err(|error| map_spawn_error(name, error))?;
    let input_task = match (input, child.stdin.take()) {
        (Some(value), Some(mut stdin)) => {
            let bytes = value_to_bytes(&value).into_bytes();
            Some(tokio::spawn(async move { stdin.write_all(&bytes).await }))
        }
        _ => None,
    };
    let output = child.wait_with_output().await?;
    if let Some(task) = input_task {
        match task
            .await
            .map_err(|error| RuntimeError::Custom(format!("pipeline input task failed: {error}")))?
        {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => {}
            Err(error) => return Err(error.into()),
        }
    }
    if emit {
        tokio::io::stdout().write_all(&output.stdout).await?;
    }

    if output.stdout.is_empty() && !output.status.success() {
        Ok(Value::Bool(false))
    } else {
        Ok(Value::String(
            String::from_utf8_lossy(&output.stdout).into_owned(),
        ))
    }
}

fn values_to_args(args: Vec<Value>) -> RuntimeResult<Vec<String>> {
    args.into_iter()
        .map(|arg| match arg {
            Value::String(value) => Ok(value),
            Value::Number(value) => Ok(value.to_string()),
            Value::Bool(value) => Ok(value.to_string()),
            other => Err(RuntimeError::TypeError {
                expected: "string, number, or bool".to_string(),
                got: other.type_name().to_string(),
            }),
        })
        .collect()
}

fn value_to_bytes(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => String::new(),
        other => format!("{other:?}"),
    }
}

fn map_spawn_error(name: &str, error: std::io::Error) -> RuntimeError {
    if error.kind() == std::io::ErrorKind::NotFound {
        RuntimeError::UndefinedCommand(name.to_string())
    } else {
        RuntimeError::IoError(error)
    }
}
