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

        let mut entries = Vec::new();

        for entry in std::fs::read_dir(&path)? {
            let entry = entry?;
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy().to_string();

            // 跳过隐藏文件
            if !show_hidden && name.starts_with('.') {
                continue;
            }

            if long_format {
                let metadata = entry.metadata()?;
                let size = metadata.len();
                let _modified = metadata.modified()?;

                let file_type = if metadata.is_dir() {
                    "dir"
                } else if metadata.is_symlink() {
                    "link"
                } else {
                    "file"
                };

                let mut row = HashMap::new();
                row.insert("name".to_string(), Value::String(name));
                row.insert("type".to_string(), Value::String(file_type.to_string()));
                row.insert("size".to_string(), Value::Number(size as f64));

                entries.push(row);
            } else {
                let mut row = HashMap::new();
                row.insert("name".to_string(), Value::String(name));
                entries.push(row);
            }
        }

        if long_format {
            // 表格格式输出
            let columns = vec!["name".to_string(), "type".to_string(), "size".to_string()];

            Ok(Value::Table {
                columns,
                rows: entries,
            })
        } else {
            // 简单列表输出
            for entry in &entries {
                if let Some(Value::String(name)) = entry.get("name") {
                    println!("{}", name);
                }
            }

            Ok(Value::Table {
                columns: vec!["name".to_string()],
                rows: entries,
            })
        }
    }
}
