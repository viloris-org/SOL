use crate::parser::Value;
use crate::runtime::{RuntimeError, RuntimeResult};
use std::collections::HashMap;
use std::process::Stdio;
use tokio::process::Command;

pub async fn execute_external(
    name: &str,
    args: Vec<Value>,
    _flags: HashMap<String, Value>,
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

    // 执行外部命令
    let output = Command::new(name)
        .args(&cmd_args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .stdin(Stdio::inherit())
        .output()
        .await
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                RuntimeError::UndefinedCommand(name.to_string())
            } else {
                RuntimeError::IoError(e)
            }
        })?;

    if output.status.success() {
        Ok(Value::Null)
    } else {
        Err(RuntimeError::Custom(format!(
            "Command '{}' failed with exit code: {}",
            name,
            output.status.code().unwrap_or(-1)
        )))
    }
}
