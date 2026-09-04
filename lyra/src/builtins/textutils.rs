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
        mut args: Vec<Value>,
        flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        let num_lines = take_numeric_flag(&mut args, &flags, "n", "lines", 10)?;

        if args.is_empty() {
            // Read from stdin
            let stdin = io::stdin();
            let reader = BufReader::new(stdin);
            let lines = reader
                .lines()
                .take(num_lines)
                .collect::<Result<Vec<_>, _>>()?;
            let output = lines_to_output(&lines);
            print!("{output}");
            return Ok(Value::String(output));
        }

        let mut output = String::new();
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

            let file = fs::File::open(expand_path(&path))
                .map_err(|e| RuntimeError::Custom(format!("head: {}: {}", path, e)))?;

            let reader = BufReader::new(file);
            let lines = reader
                .lines()
                .take(num_lines)
                .collect::<Result<Vec<_>, _>>()?;
            output.push_str(&lines_to_output(&lines));
        }

        print!("{output}");
        Ok(Value::String(output))
    }

    async fn execute_piped(
        &self,
        mut args: Vec<Value>,
        flags: HashMap<String, Value>,
        input: Option<Value>,
        emit: bool,
    ) -> RuntimeResult<Value> {
        let num_lines = take_numeric_flag(&mut args, &flags, "n", "lines", 10)?;
        let lines = read_lines(args, input, "head")?
            .into_iter()
            .take(num_lines)
            .collect::<Vec<_>>();
        let output = lines_to_output(&lines);
        if emit {
            print!("{output}");
        }
        Ok(Value::String(output))
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
        mut args: Vec<Value>,
        flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        let num_lines = take_numeric_flag(&mut args, &flags, "n", "lines", 10)?;

        if args.is_empty() {
            // Read from stdin
            let stdin = io::stdin();
            let reader = BufReader::new(stdin);
            let lines: Vec<String> = reader.lines().collect::<Result<Vec<_>, _>>()?;
            let start = lines.len().saturating_sub(num_lines);
            let output = lines_to_output(&lines[start..]);
            print!("{output}");
            return Ok(Value::String(output));
        }

        let mut output = String::new();
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

            let file = fs::File::open(expand_path(&path))
                .map_err(|e| RuntimeError::Custom(format!("tail: {}: {}", path, e)))?;

            let reader = BufReader::new(file);
            let lines: Vec<String> = reader.lines().collect::<Result<Vec<_>, _>>()?;
            let start = lines.len().saturating_sub(num_lines);
            output.push_str(&lines_to_output(&lines[start..]));
        }

        print!("{output}");
        Ok(Value::String(output))
    }

    async fn execute_piped(
        &self,
        mut args: Vec<Value>,
        flags: HashMap<String, Value>,
        input: Option<Value>,
        emit: bool,
    ) -> RuntimeResult<Value> {
        let num_lines = take_numeric_flag(&mut args, &flags, "n", "lines", 10)?;
        let lines = read_lines(args, input, "tail")?;
        let start = lines.len().saturating_sub(num_lines);
        let output = lines_to_output(&lines[start..]);
        if emit {
            print!("{output}");
        }
        Ok(Value::String(output))
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
        let count_chars = flags.get("m").or(flags.get("chars")).is_some();
        let count_bytes = flags.get("c").or(flags.get("bytes")).is_some();

        // If no flags specified, show all counts
        let show_all = !count_lines && !count_words && !count_chars && !count_bytes;

        if args.is_empty() {
            // Read from stdin
            let mut buffer = String::new();
            io::stdin().read_to_string(&mut buffer)?;
            let output = format_counts(
                &buffer,
                "stdin",
                show_all,
                count_lines,
                count_words,
                count_chars,
                count_bytes,
            );
            print!("{output}");
            return Ok(Value::String(output));
        }

        let mut output = String::new();
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

            let content = fs::read_to_string(expand_path(&path))
                .map_err(|e| RuntimeError::Custom(format!("wc: {}: {}", path, e)))?;

            output.push_str(&format_counts(
                &content,
                &path,
                show_all,
                count_lines,
                count_words,
                count_chars,
                count_bytes,
            ));
        }

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
        let count_lines = flags.get("l").or(flags.get("lines")).is_some();
        let count_words = flags.get("w").or(flags.get("words")).is_some();
        let count_chars = flags.get("m").or(flags.get("chars")).is_some();
        let count_bytes = flags.get("c").or(flags.get("bytes")).is_some();
        let show_all = !count_lines && !count_words && !count_chars && !count_bytes;
        let text = read_text(args, input, "wc")?;
        let output = format_counts(
            &text,
            "stdin",
            show_all,
            count_lines,
            count_words,
            count_chars,
            count_bytes,
        );
        if emit {
            print!("{output}");
        }
        Ok(Value::String(output))
    }
}

fn format_counts(
    content: &str,
    name: &str,
    show_all: bool,
    lines: bool,
    words: bool,
    chars: bool,
    bytes: bool,
) -> String {
    let line_count = content.bytes().filter(|byte| *byte == b'\n').count();
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

    format!("{output} {name}\n")
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
            let output = grep_lines(
                reader.lines(),
                &pattern,
                ignore_case,
                invert,
                line_numbers,
                count_only,
                None,
            )?;
            print!("{output}");
            return Ok(Value::String(output));
        }

        let mut output = String::new();
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

            let file = fs::File::open(expand_path(path))
                .map_err(|e| RuntimeError::Custom(format!("grep: {}: {}", path, e)))?;

            let reader = BufReader::new(file);
            let filename = if files.len() > 1 {
                Some(path.as_str())
            } else {
                None
            };
            output.push_str(&grep_lines(
                reader.lines(),
                &pattern,
                ignore_case,
                invert,
                line_numbers,
                count_only,
                filename,
            )?);
        }

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
        if args.is_empty() {
            return Err(RuntimeError::Custom("grep: missing pattern".to_string()));
        }
        let Value::String(pattern) = &args[0] else {
            return Err(RuntimeError::TypeError {
                expected: "string".to_string(),
                got: args[0].type_name().to_string(),
            });
        };
        let files = args[1..].to_vec();
        let text = read_text(files, input, "grep")?;
        let output = grep_lines(
            text.lines().map(|line| Ok(line.to_string())),
            pattern,
            flags.get("i").or(flags.get("ignore-case")).is_some(),
            flags.get("v").or(flags.get("invert-match")).is_some(),
            flags.get("n").or(flags.get("line-number")).is_some(),
            flags.get("c").or(flags.get("count")).is_some(),
            None,
        )?;
        if emit {
            print!("{output}");
        }
        Ok(Value::String(output))
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
) -> RuntimeResult<String>
where
    I: Iterator<Item = io::Result<String>>,
{
    let pattern_lower = pattern.to_lowercase();
    let mut match_count = 0;
    let mut output = String::new();

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
                output.push_str(&prefix);
                output.push_str(&line);
                output.push('\n');
            }
        }
    }

    if count_only {
        if let Some(f) = filename {
            output.push_str(&format!("{f}:{match_count}\n"));
        } else {
            output.push_str(&format!("{match_count}\n"));
        }
    }

    Ok(output)
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

                let content = fs::read_to_string(expand_path(&path))
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

        let output = lines_to_output(&lines);
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
        let mut lines = read_lines(args, input, "sort")?;
        sort_lines(&mut lines, &flags);
        let output = lines_to_output(&lines);
        if emit {
            print!("{output}");
        }
        Ok(Value::String(output))
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

            let content = fs::read_to_string(expand_path(path))
                .map_err(|e| RuntimeError::Custom(format!("uniq: {}: {}", path, e)))?;

            lines.extend(content.lines().map(|s| s.to_string()));
        }

        let output = uniq_output(lines, count, duplicates_only, unique_only);
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
        let lines = read_lines(args, input, "uniq")?;
        let output = uniq_output(
            lines,
            flags.get("c").or(flags.get("count")).is_some(),
            flags.get("d").or(flags.get("repeated")).is_some(),
            flags.get("u").or(flags.get("unique")).is_some(),
        );
        if emit {
            print!("{output}");
        }
        Ok(Value::String(output))
    }
}

fn uniq_output(
    lines: Vec<String>,
    show_count: bool,
    duplicates_only: bool,
    unique_only: bool,
) -> String {
    let mut output = String::new();
    let mut current: Option<(String, usize)> = None;

    for line in lines {
        match &mut current {
            Some((previous, count)) if previous == &line => *count += 1,
            Some((previous, count)) => {
                push_uniq_line(
                    &mut output,
                    previous,
                    *count,
                    show_count,
                    duplicates_only,
                    unique_only,
                );
                *previous = line;
                *count = 1;
            }
            None => current = Some((line, 1)),
        }
    }
    if let Some((line, count)) = current {
        push_uniq_line(
            &mut output,
            &line,
            count,
            show_count,
            duplicates_only,
            unique_only,
        );
    }
    output
}

fn push_uniq_line(
    output: &mut String,
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
        output.push_str(&format!("{line_count:7} {line}\n"));
    } else {
        output.push_str(line);
        output.push('\n');
    }
}

fn sort_lines(lines: &mut Vec<String>, flags: &HashMap<String, Value>) {
    let reverse = flags.get("r").or(flags.get("reverse")).is_some();
    let unique = flags.get("u").or(flags.get("unique")).is_some();
    let numeric = flags.get("n").or(flags.get("numeric-sort")).is_some();

    if numeric {
        lines.sort_by(|a, b| {
            let a_num: Result<f64, _> = a.trim().parse();
            let b_num: Result<f64, _> = b.trim().parse();
            match (a_num, b_num) {
                (Ok(left), Ok(right)) => left
                    .partial_cmp(&right)
                    .unwrap_or(std::cmp::Ordering::Equal),
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
}

fn take_numeric_flag(
    args: &mut Vec<Value>,
    flags: &HashMap<String, Value>,
    short: &str,
    long: &str,
    default: usize,
) -> RuntimeResult<usize> {
    let value = flags.get(short).or_else(|| flags.get(long));
    match value {
        None => Ok(default),
        Some(Value::Number(number)) if *number >= 0.0 => Ok(*number as usize),
        Some(Value::Bool(true)) if !args.is_empty() => {
            let raw = match args.remove(0) {
                Value::String(value) => value,
                Value::Number(value) if value >= 0.0 => return Ok(value as usize),
                value => {
                    return Err(RuntimeError::TypeError {
                        expected: "non-negative line count".to_string(),
                        got: value.type_name().to_string(),
                    });
                }
            };
            let count = raw
                .parse::<usize>()
                .map_err(|_| RuntimeError::Custom(format!("invalid number of lines: '{raw}'")))?;
            Ok(count)
        }
        Some(value) => Err(RuntimeError::TypeError {
            expected: "non-negative line count".to_string(),
            got: value.type_name().to_string(),
        }),
    }
}

fn lines_to_output(lines: &[String]) -> String {
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
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

fn read_text(args: Vec<Value>, input: Option<Value>, command: &str) -> RuntimeResult<String> {
    if !args.is_empty() {
        let mut output = String::new();
        for arg in args {
            let Value::String(path) = arg else {
                return Err(RuntimeError::TypeError {
                    expected: "string".to_string(),
                    got: arg.type_name().to_string(),
                });
            };
            output
                .push_str(&fs::read_to_string(expand_path(&path)).map_err(|error| {
                    RuntimeError::Custom(format!("{command}: {path}: {error}"))
                })?);
        }
        return Ok(output);
    }

    if let Some(value) = input {
        return Ok(value_as_text(value));
    }

    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;
    Ok(buffer)
}

fn read_lines(args: Vec<Value>, input: Option<Value>, command: &str) -> RuntimeResult<Vec<String>> {
    Ok(read_text(args, input, command)?
        .lines()
        .map(str::to_string)
        .collect())
}

fn expand_path(path: &str) -> std::path::PathBuf {
    if path == "~" {
        return std::env::var_os("HOME").map_or_else(|| path.into(), Into::into);
    }
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return std::path::PathBuf::from(home).join(rest);
    }
    path.into()
}
