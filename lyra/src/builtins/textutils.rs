use crate::builtins::Builtin;
use crate::parser::Value;
use crate::runtime::{RuntimeError, RuntimeResult};
use async_trait::async_trait;
use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, BufReader, Read};

/// head - output the first part of files
pub struct Head;

#[async_trait]
impl Builtin for Head {
    fn name(&self) -> &str {
        "head"
    }

    fn description(&self) -> &str {
        "Output the first part of files"
    }

    async fn execute(
        &self,
        args: Vec<Value>,
        flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        let num_lines = flags
            .get("n")
            .or(flags.get("lines"))
            .and_then(|v| match v {
                Value::Number(n) => Some(*n as usize),
                _ => None,
            })
            .unwrap_or(10);

        if args.is_empty() {
            // Read from stdin
            let stdin = io::stdin();
            let reader = BufReader::new(stdin);
            for (i, line) in reader.lines().enumerate() {
                if i >= num_lines {
                    break;
                }
                println!("{}", line?);
            }
            return Ok(Value::Null);
        }

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

            let file = fs::File::open(&path)
                .map_err(|e| RuntimeError::Custom(format!("head: {}: {}", path, e)))?;

            let reader = BufReader::new(file);
            for (i, line) in reader.lines().enumerate() {
                if i >= num_lines {
                    break;
                }
                println!("{}", line?);
            }
        }

        Ok(Value::Null)
    }
}

/// tail - output the last part of files
pub struct Tail;

#[async_trait]
impl Builtin for Tail {
    fn name(&self) -> &str {
        "tail"
    }

    fn description(&self) -> &str {
        "Output the last part of files"
    }

    async fn execute(
        &self,
        args: Vec<Value>,
        flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        let num_lines = flags
            .get("n")
            .or(flags.get("lines"))
            .and_then(|v| match v {
                Value::Number(n) => Some(*n as usize),
                _ => None,
            })
            .unwrap_or(10);

        if args.is_empty() {
            // Read from stdin
            let stdin = io::stdin();
            let reader = BufReader::new(stdin);
            let lines: Vec<String> = reader.lines().collect::<Result<Vec<_>, _>>()?;
            let start = lines.len().saturating_sub(num_lines);
            for line in &lines[start..] {
                println!("{}", line);
            }
            return Ok(Value::Null);
        }

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

            let file = fs::File::open(&path)
                .map_err(|e| RuntimeError::Custom(format!("tail: {}: {}", path, e)))?;

            let reader = BufReader::new(file);
            let lines: Vec<String> = reader.lines().collect::<Result<Vec<_>, _>>()?;
            let start = lines.len().saturating_sub(num_lines);
            for line in &lines[start..] {
                println!("{}", line);
            }
        }

        Ok(Value::Null)
    }
}

/// wc - word, line, character, and byte count
pub struct Wc;

#[async_trait]
impl Builtin for Wc {
    fn name(&self) -> &str {
        "wc"
    }

    fn description(&self) -> &str {
        "Count lines, words, and characters"
    }

    async fn execute(
        &self,
        args: Vec<Value>,
        flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        let count_lines = flags.get("l").or(flags.get("lines")).is_some();
        let count_words = flags.get("w").or(flags.get("words")).is_some();
        let count_chars = flags.get("c").or(flags.get("chars")).is_some();
        let count_bytes = flags.get("m").or(flags.get("bytes")).is_some();

        // If no flags specified, show all counts
        let show_all = !count_lines && !count_words && !count_chars && !count_bytes;

        if args.is_empty() {
            // Read from stdin
            let mut buffer = String::new();
            io::stdin().read_to_string(&mut buffer)?;
            print_counts(
                &buffer,
                "stdin",
                show_all,
                count_lines,
                count_words,
                count_chars,
                count_bytes,
            );
            return Ok(Value::Null);
        }

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
                .map_err(|e| RuntimeError::Custom(format!("wc: {}: {}", path, e)))?;

            print_counts(
                &content,
                &path,
                show_all,
                count_lines,
                count_words,
                count_chars,
                count_bytes,
            );
        }

        Ok(Value::Null)
    }
}

fn print_counts(
    content: &str,
    name: &str,
    show_all: bool,
    lines: bool,
    words: bool,
    chars: bool,
    bytes: bool,
) {
    let line_count = content.lines().count();
    let word_count = content.split_whitespace().count();
    let char_count = content.chars().count();
    let byte_count = content.len();

    let mut output = String::new();

    if show_all || lines {
        output.push_str(&format!("{:8}", line_count));
    }
    if show_all || words {
        output.push_str(&format!("{:8}", word_count));
    }
    if show_all || chars {
        output.push_str(&format!("{:8}", char_count));
    }
    if bytes {
        output.push_str(&format!("{:8}", byte_count));
    }

    println!("{} {}", output, name);
}

/// grep - search text using patterns
pub struct Grep;

#[async_trait]
impl Builtin for Grep {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Search for patterns in files"
    }

    async fn execute(
        &self,
        args: Vec<Value>,
        flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        if args.is_empty() {
            return Err(RuntimeError::Custom("grep: missing pattern".to_string()));
        }

        let pattern = match &args[0] {
            Value::String(s) => s.clone(),
            _ => {
                return Err(RuntimeError::TypeError {
                    expected: "string".to_string(),
                    got: args[0].type_name().to_string(),
                });
            }
        };

        let ignore_case = flags.get("i").or(flags.get("ignore-case")).is_some();
        let invert = flags.get("v").or(flags.get("invert-match")).is_some();
        let line_numbers = flags.get("n").or(flags.get("line-number")).is_some();
        let count_only = flags.get("c").or(flags.get("count")).is_some();

        let files = &args[1..];

        if files.is_empty() {
            // Read from stdin
            let stdin = io::stdin();
            let reader = BufReader::new(stdin);
            grep_lines(
                reader.lines(),
                &pattern,
                ignore_case,
                invert,
                line_numbers,
                count_only,
                None,
            )?;
            return Ok(Value::Null);
        }

        for file_val in files {
            let path = match file_val {
                Value::String(s) => s,
                _ => {
                    return Err(RuntimeError::TypeError {
                        expected: "string".to_string(),
                        got: file_val.type_name().to_string(),
                    });
                }
            };

            let file = fs::File::open(path)
                .map_err(|e| RuntimeError::Custom(format!("grep: {}: {}", path, e)))?;

            let reader = BufReader::new(file);
            let filename = if files.len() > 1 {
                Some(path.as_str())
            } else {
                None
            };
            grep_lines(
                reader.lines(),
                &pattern,
                ignore_case,
                invert,
                line_numbers,
                count_only,
                filename,
            )?;
        }

        Ok(Value::Null)
    }
}

fn grep_lines<I>(
    lines: I,
    pattern: &str,
    ignore_case: bool,
    invert: bool,
    line_numbers: bool,
    count_only: bool,
    filename: Option<&str>,
) -> RuntimeResult<()>
where
    I: Iterator<Item = io::Result<String>>,
{
    let pattern_lower = pattern.to_lowercase();
    let mut match_count = 0;

    for (line_num, line_result) in lines.enumerate() {
        let line = line_result?;
        let matches = if ignore_case {
            line.to_lowercase().contains(&pattern_lower)
        } else {
            line.contains(pattern)
        };

        let should_print = if invert { !matches } else { matches };

        if should_print {
            match_count += 1;
            if !count_only {
                let prefix = match (filename, line_numbers) {
                    (Some(f), true) => format!("{}:{}:", f, line_num + 1),
                    (Some(f), false) => format!("{}:", f),
                    (None, true) => format!("{}:", line_num + 1),
                    (None, false) => String::new(),
                };
                println!("{}{}", prefix, line);
            }
        }
    }

    if count_only {
        if let Some(f) = filename {
            println!("{}:{}", f, match_count);
        } else {
            println!("{}", match_count);
        }
    }

    Ok(())
}

/// sort - sort lines of text
pub struct Sort;

#[async_trait]
impl Builtin for Sort {
    fn name(&self) -> &str {
        "sort"
    }

    fn description(&self) -> &str {
        "Sort lines of text files"
    }

    async fn execute(
        &self,
        args: Vec<Value>,
        flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        let reverse = flags.get("r").or(flags.get("reverse")).is_some();
        let unique = flags.get("u").or(flags.get("unique")).is_some();
        let numeric = flags.get("n").or(flags.get("numeric-sort")).is_some();

        let mut lines = Vec::new();

        if args.is_empty() {
            // Read from stdin
            let stdin = io::stdin();
            let reader = BufReader::new(stdin);
            for line in reader.lines() {
                lines.push(line?);
            }
        } else {
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
                    .map_err(|e| RuntimeError::Custom(format!("sort: {}: {}", path, e)))?;

                lines.extend(content.lines().map(|s| s.to_string()));
            }
        }

        if numeric {
            lines.sort_by(|a, b| {
                let a_num: Result<f64, _> = a.trim().parse();
                let b_num: Result<f64, _> = b.trim().parse();
                match (a_num, b_num) {
                    (Ok(an), Ok(bn)) => an.partial_cmp(&bn).unwrap_or(std::cmp::Ordering::Equal),
                    _ => a.cmp(b),
                }
            });
        } else {
            lines.sort();
        }

        if reverse {
            lines.reverse();
        }

        if unique {
            lines.dedup();
        }

        for line in lines {
            println!("{}", line);
        }

        Ok(Value::Null)
    }
}

/// uniq - report or omit repeated lines
pub struct Uniq;

#[async_trait]
impl Builtin for Uniq {
    fn name(&self) -> &str {
        "uniq"
    }

    fn description(&self) -> &str {
        "Report or filter out repeated lines"
    }

    async fn execute(
        &self,
        args: Vec<Value>,
        flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        let count = flags.get("c").or(flags.get("count")).is_some();
        let duplicates_only = flags.get("d").or(flags.get("repeated")).is_some();
        let unique_only = flags.get("u").or(flags.get("unique")).is_some();

        let mut lines = Vec::new();

        if args.is_empty() {
            // Read from stdin
            let stdin = io::stdin();
            let reader = BufReader::new(stdin);
            for line in reader.lines() {
                lines.push(line?);
            }
        } else {
            let path = match &args[0] {
                Value::String(s) => s,
                _ => {
                    return Err(RuntimeError::TypeError {
                        expected: "string".to_string(),
                        got: args[0].type_name().to_string(),
                    });
                }
            };

            let content = fs::read_to_string(path)
                .map_err(|e| RuntimeError::Custom(format!("uniq: {}: {}", path, e)))?;

            lines.extend(content.lines().map(|s| s.to_string()));
        }

        let mut current: Option<(String, usize)> = None;

        for line in lines {
            match &mut current {
                Some((prev_line, count_val)) if prev_line == &line => {
                    *count_val += 1;
                }
                Some((prev_line, count_val)) => {
                    print_uniq_line(prev_line, *count_val, count, duplicates_only, unique_only);
                    *prev_line = line;
                    *count_val = 1;
                }
                None => {
                    current = Some((line, 1));
                }
            }
        }

        if let Some((line, count_val)) = current {
            print_uniq_line(&line, count_val, count, duplicates_only, unique_only);
        }

        Ok(Value::Null)
    }
}

fn print_uniq_line(
    line: &str,
    line_count: usize,
    show_count: bool,
    duplicates_only: bool,
    unique_only: bool,
) {
    let is_duplicate = line_count > 1;
    let is_unique = line_count == 1;

    if duplicates_only && !is_duplicate {
        return;
    }
    if unique_only && !is_unique {
        return;
    }

    if show_count {
        println!("{:7} {}", line_count, line);
    } else {
        println!("{}", line);
    }
}
