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
        let show_line_numbers = flags.get("n").or(flags.get("number")).is_some();
        let show_ends = flags.get("E").or(flags.get("show-ends")).is_some();

        if args.is_empty() {
            // Read from stdin
            let mut buffer = String::new();
            io::stdin().read_to_string(&mut buffer)?;
            print!("{}", buffer);
            return Ok(Value::String(buffer));
        }

        let mut all_content = String::new();

        for arg in args {
            let path = match arg {
                Value::String(s) => s,
                _ => {
                    return Err(RuntimeError::TypeError {
                        expected: "string".to_string(),
                        got: arg.type_name().to_string(),
                    });
                }
            };

            let content = fs::read_to_string(&path)
                .map_err(|e| RuntimeError::Custom(format!("cat: {}: {}", path, e)))?;

            if show_line_numbers {
                for (i, line) in content.lines().enumerate() {
                    let line_str = if show_ends {
                        format!("{}$", line)
                    } else {
                        line.to_string()
                    };
                    println!("{:6}\t{}", i + 1, line_str);
                }
            } else if show_ends {
                for line in content.lines() {
                    println!("{}$", line);
                }
            } else {
                print!("{}", content);
            }

            all_content.push_str(&content);
        }

        Ok(Value::String(all_content))
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

        let dest = PathBuf::from(&dest_str);
        let sources: Vec<PathBuf> = args[..args.len() - 1]
            .iter()
            .map(|v| match v {
                Value::String(s) => Ok(PathBuf::from(s)),
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
                dest.join(source.file_name().unwrap())
            } else {
                dest.clone()
            };

            if source.is_dir() {
                if !recursive {
                    eprintln!(
                        "cp: -r not specified; omitting directory '{}'",
                        source.display()
                    );
                    continue;
                }
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

        if src_path.is_dir() {
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

        let dest = PathBuf::from(&dest_str);
        let sources: Vec<PathBuf> = args[..args.len() - 1]
            .iter()
            .map(|v| match v {
                Value::String(s) => Ok(PathBuf::from(s)),
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
                dest.join(source.file_name().unwrap())
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

            let path = PathBuf::from(&path_str);

            if !path.exists() {
                if !force {
                    eprintln!(
                        "rm: cannot remove '{}': No such file or directory",
                        path_str
                    );
                }
                continue;
            }

            if path.is_dir() {
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

            let path = PathBuf::from(&path_str);

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

            let path = PathBuf::from(&path_str);

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
