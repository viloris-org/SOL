use crate::builtins::Builtin;
use crate::parser::Value;
use crate::runtime::{RuntimeError, RuntimeResult};
use async_trait::async_trait;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// cat - concatenate and display files
pub struct Cat;

#[async_trait]
impl Builtin for Cat {
    fn name(&self) -> &str {
        "cat"
    }

    fn description(&self) -> &str {
        "Concatenate and display file contents"
    }

    async fn execute(
        &self,
        args: Vec<Value>,
        flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        let output = cat_output(args, &flags, None)?;
        print!("{output}");
        Ok(Value::String(output))
    }

    async fn execute_piped(
        &self,
        args: Vec<Value>,
        flags: HashMap<String, Value>,
        input: Option<Value>,
        emit: bool,
    ) -> RuntimeResult<Value> {
        let output = cat_output(args, &flags, input)?;
        if emit {
            print!("{output}");
        }
        Ok(Value::String(output))
    }
}

fn cat_output(
    args: Vec<Value>,
    flags: &HashMap<String, Value>,
    input: Option<Value>,
) -> RuntimeResult<String> {
    let show_line_numbers = flags.get("n").or(flags.get("number")).is_some();
    let show_ends = flags.get("E").or(flags.get("show-ends")).is_some();

    let contents = if args.is_empty() {
        vec![if let Some(value) = input {
            value_as_text(value)
        } else {
            let mut buffer = String::new();
            io::stdin().read_to_string(&mut buffer)?;
            buffer
        }]
    } else {
        args.into_iter()
            .map(|arg| {
                let Value::String(path) = arg else {
                    return Err(RuntimeError::TypeError {
                        expected: "string".to_string(),
                        got: arg.type_name().to_string(),
                    });
                };
                fs::read_to_string(expand_path(&path))
                    .map_err(|error| RuntimeError::Custom(format!("cat: {path}: {error}")))
            })
            .collect::<RuntimeResult<Vec<_>>>()?
    };

    let mut output = String::new();
    for content in contents {
        if show_line_numbers || show_ends {
            for (index, line) in content.lines().enumerate() {
                if show_line_numbers {
                    output.push_str(&format!("{:6}\t", index + 1));
                }
                output.push_str(line);
                if show_ends {
                    output.push('$');
                }
                output.push('\n');
            }
        } else {
            output.push_str(&content);
        }
    }
    Ok(output)
}

fn value_as_text(value: Value) -> String {
    match value {
        Value::String(value) => value,
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => String::new(),
        other => format!("{other:?}"),
    }
}

/// cp - copy files and directories
pub struct Cp;

#[async_trait]
impl Builtin for Cp {
    fn name(&self) -> &str {
        "cp"
    }

    fn description(&self) -> &str {
        "Copy files and directories"
    }

    async fn execute(
        &self,
        args: Vec<Value>,
        flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        if args.len() < 2 {
            return Err(RuntimeError::Custom("cp: missing file operand".to_string()));
        }

        let recursive = flags.get("r").or(flags.get("recursive")).is_some();
        let force = flags.get("f").or(flags.get("force")).is_some();
        let verbose = flags.get("v").or(flags.get("verbose")).is_some();

        let dest_str = match args.last().unwrap() {
            Value::String(s) => s.clone(),
            _ => {
                return Err(RuntimeError::TypeError {
                    expected: "string".to_string(),
                    got: args.last().unwrap().type_name().to_string(),
                });
            }
        };

        let dest = expand_path(&dest_str);
        let sources: Vec<PathBuf> = args[..args.len() - 1]
            .iter()
            .map(|v| match v {
                Value::String(s) => Ok(expand_path(s)),
                _ => Err(RuntimeError::TypeError {
                    expected: "string".to_string(),
                    got: v.type_name().to_string(),
                }),
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Multiple sources or dest is directory → copy into dest
        let dest_is_dir = dest.is_dir();
        let multiple_sources = sources.len() > 1;

        if multiple_sources && !dest_is_dir {
            return Err(RuntimeError::Custom(format!(
                "cp: target '{}' is not a directory",
                dest_str
            )));
        }

        for source in sources {
            let target = if dest_is_dir {
                let file_name = source.file_name().ok_or_else(|| {
                    RuntimeError::Custom(format!(
                        "cp: cannot derive a target name for '{}'",
                        source.display()
                    ))
                })?;
                dest.join(file_name)
            } else {
                dest.clone()
            };

            let metadata = fs::symlink_metadata(&source).map_err(|error| {
                RuntimeError::Custom(format!("cp: cannot stat '{}': {error}", source.display()))
            })?;
            if metadata.file_type().is_symlink() {
                copy_symlink(&source, &target, force)?;
            } else if metadata.is_dir() {
                if !recursive {
                    eprintln!(
                        "cp: -r not specified; omitting directory '{}'",
                        source.display()
                    );
                    continue;
                }
                ensure_copy_target_outside_source(&source, &target)?;
                copy_dir_recursive(&source, &target, force, verbose)?;
            } else {
                if !force && target.exists() {
                    eprintln!("cp: not overwriting '{}' without -f", target.display());
                    continue;
                }
                fs::copy(&source, &target).map_err(|e| {
                    RuntimeError::Custom(format!(
                        "cp: cannot copy '{}' to '{}': {}",
                        source.display(),
                        target.display(),
                        e
                    ))
                })?;
                if verbose {
                    println!("'{}' -> '{}'", source.display(), target.display());
                }
            }
        }

        Ok(Value::Null)
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path, force: bool, verbose: bool) -> RuntimeResult<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        let metadata = fs::symlink_metadata(&src_path)?;
        if metadata.file_type().is_symlink() {
            copy_symlink(&src_path, &dst_path, force)?;
        } else if metadata.is_dir() {
            copy_dir_recursive(&src_path, &dst_path, force, verbose)?;
        } else {
            if !force && dst_path.exists() {
                continue;
            }
            fs::copy(&src_path, &dst_path)?;
            if verbose {
                println!("'{}' -> '{}'", src_path.display(), dst_path.display());
            }
        }
    }
    Ok(())
}

fn ensure_copy_target_outside_source(src: &Path, dst: &Path) -> RuntimeResult<()> {
    let source = fs::canonicalize(src)?;
    let target = resolve_path_for_comparison(dst)?;

    if target.starts_with(&source) {
        return Err(RuntimeError::Custom(format!(
            "cp: cannot copy '{}' into itself ('{}')",
            src.display(),
            dst.display()
        )));
    }
    Ok(())
}

fn resolve_path_for_comparison(path: &Path) -> RuntimeResult<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let normalized = normalize_path(&absolute);
    let mut cursor = normalized.as_path();
    let mut missing = Vec::new();

    while fs::symlink_metadata(cursor).is_err() {
        if let Some(name) = cursor.file_name() {
            missing.push(name.to_os_string());
        }
        cursor = cursor.parent().ok_or_else(|| {
            RuntimeError::Custom(format!("cannot resolve path '{}'", path.display()))
        })?;
    }

    let mut resolved = fs::canonicalize(cursor)?;
    for component in missing.iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

#[cfg(unix)]
fn copy_symlink(src: &Path, dst: &Path, force: bool) -> RuntimeResult<()> {
    use std::os::unix::fs::symlink;

    if dst.symlink_metadata().is_ok() {
        if !force {
            return Ok(());
        }
        if dst.is_dir() && !dst.is_symlink() {
            fs::remove_dir_all(dst)?;
        } else {
            fs::remove_file(dst)?;
        }
    }
    symlink(fs::read_link(src)?, dst)?;
    Ok(())
}

#[cfg(not(unix))]
fn copy_symlink(src: &Path, dst: &Path, force: bool) -> RuntimeResult<()> {
    if !force && dst.exists() {
        return Ok(());
    }
    fs::copy(src, dst)?;
    Ok(())
}

/// mv - move (rename) files
pub struct Mv;

#[async_trait]
impl Builtin for Mv {
    fn name(&self) -> &str {
        "mv"
    }

    fn description(&self) -> &str {
        "Move or rename files and directories"
    }

    async fn execute(
        &self,
        args: Vec<Value>,
        flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        if args.len() < 2 {
            return Err(RuntimeError::Custom("mv: missing file operand".to_string()));
        }

        let force = flags.get("f").or(flags.get("force")).is_some();
        let verbose = flags.get("v").or(flags.get("verbose")).is_some();

        let dest_str = match args.last().unwrap() {
            Value::String(s) => s.clone(),
            _ => {
                return Err(RuntimeError::TypeError {
                    expected: "string".to_string(),
                    got: args.last().unwrap().type_name().to_string(),
                });
            }
        };

        let dest = expand_path(&dest_str);
        let sources: Vec<PathBuf> = args[..args.len() - 1]
            .iter()
            .map(|v| match v {
                Value::String(s) => Ok(expand_path(s)),
                _ => Err(RuntimeError::TypeError {
                    expected: "string".to_string(),
                    got: v.type_name().to_string(),
                }),
            })
            .collect::<Result<Vec<_>, _>>()?;

        let dest_is_dir = dest.is_dir();
        let multiple_sources = sources.len() > 1;

        if multiple_sources && !dest_is_dir {
            return Err(RuntimeError::Custom(format!(
                "mv: target '{}' is not a directory",
                dest_str
            )));
        }

        for source in sources {
            let target = if dest_is_dir {
                let file_name = source.file_name().ok_or_else(|| {
                    RuntimeError::Custom(format!(
                        "mv: cannot derive a target name for '{}'",
                        source.display()
                    ))
                })?;
                dest.join(file_name)
            } else {
                dest.clone()
            };

            if !force && target.exists() {
                eprintln!("mv: not overwriting '{}' without -f", target.display());
                continue;
            }

            fs::rename(&source, &target).map_err(|e| {
                RuntimeError::Custom(format!(
                    "mv: cannot move '{}' to '{}': {}",
                    source.display(),
                    target.display(),
                    e
                ))
            })?;

            if verbose {
                println!("'{}' -> '{}'", source.display(), target.display());
            }
        }

        Ok(Value::Null)
    }
}

/// rm - remove files or directories
pub struct Rm;

#[async_trait]
impl Builtin for Rm {
    fn name(&self) -> &str {
        "rm"
    }

    fn description(&self) -> &str {
        "Remove files or directories"
    }

    async fn execute(
        &self,
        args: Vec<Value>,
        flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        if args.is_empty() {
            return Err(RuntimeError::Custom("rm: missing operand".to_string()));
        }

        let recursive = flags.get("r").or(flags.get("recursive")).is_some();
        let force = flags.get("f").or(flags.get("force")).is_some();
        let verbose = flags.get("v").or(flags.get("verbose")).is_some();

        for arg in args {
            let path_str = match arg {
                Value::String(s) => s,
                _ => {
                    return Err(RuntimeError::TypeError {
                        expected: "string".to_string(),
                        got: arg.type_name().to_string(),
                    });
                }
            };

            let path = expand_path(&path_str);

            let Ok(metadata) = fs::symlink_metadata(&path) else {
                if !force {
                    eprintln!(
                        "rm: cannot remove '{}': No such file or directory",
                        path_str
                    );
                }
                continue;
            };

            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                if !recursive {
                    eprintln!("rm: cannot remove '{}': Is a directory", path_str);
                    continue;
                }
                fs::remove_dir_all(&path).map_err(|e| {
                    RuntimeError::Custom(format!("rm: cannot remove '{}': {}", path_str, e))
                })?;
            } else {
                fs::remove_file(&path).map_err(|e| {
                    RuntimeError::Custom(format!("rm: cannot remove '{}': {}", path_str, e))
                })?;
            }

            if verbose {
                println!("removed '{}'", path_str);
            }
        }

        Ok(Value::Null)
    }
}

/// mkdir - make directories
pub struct Mkdir;

#[async_trait]
impl Builtin for Mkdir {
    fn name(&self) -> &str {
        "mkdir"
    }

    fn description(&self) -> &str {
        "Create directories"
    }

    async fn execute(
        &self,
        args: Vec<Value>,
        flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        if args.is_empty() {
            return Err(RuntimeError::Custom("mkdir: missing operand".to_string()));
        }

        let parents = flags.get("p").or(flags.get("parents")).is_some();
        let verbose = flags.get("v").or(flags.get("verbose")).is_some();

        for arg in args {
            let path_str = match arg {
                Value::String(s) => s,
                _ => {
                    return Err(RuntimeError::TypeError {
                        expected: "string".to_string(),
                        got: arg.type_name().to_string(),
                    });
                }
            };

            let path = expand_path(&path_str);

            let result = if parents {
                fs::create_dir_all(&path)
            } else {
                fs::create_dir(&path)
            };

            result.map_err(|e| {
                RuntimeError::Custom(format!(
                    "mkdir: cannot create directory '{}': {}",
                    path_str, e
                ))
            })?;

            if verbose {
                println!("mkdir: created directory '{}'", path_str);
            }
        }

        Ok(Value::Null)
    }
}

/// touch - create empty files or update timestamps
pub struct Touch;

#[async_trait]
impl Builtin for Touch {
    fn name(&self) -> &str {
        "touch"
    }

    fn description(&self) -> &str {
        "Create empty files or update timestamps"
    }

    async fn execute(
        &self,
        args: Vec<Value>,
        _flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        if args.is_empty() {
            return Err(RuntimeError::Custom(
                "touch: missing file operand".to_string(),
            ));
        }

        for arg in args {
            let path_str = match arg {
                Value::String(s) => s,
                _ => {
                    return Err(RuntimeError::TypeError {
                        expected: "string".to_string(),
                        got: arg.type_name().to_string(),
                    });
                }
            };

            let path = expand_path(&path_str);

            if path.exists() {
                // Update timestamp by setting access/modification times
                let now = filetime::FileTime::now();
                filetime::set_file_times(&path, now, now).map_err(|e| {
                    RuntimeError::Custom(format!("touch: cannot touch '{}': {}", path_str, e))
                })?;
            } else {
                // Create empty file
                fs::File::create(&path).map_err(|e| {
                    RuntimeError::Custom(format!("touch: cannot touch '{}': {}", path_str, e))
                })?;
            }
        }

        Ok(Value::Null)
    }
}

fn expand_path(path: &str) -> PathBuf {
    if path == "~" {
        return std::env::var_os("HOME").map_or_else(|| PathBuf::from(path), PathBuf::from);
    }
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(path)
}
