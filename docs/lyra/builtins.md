# Lyra Built-in Command Reference

Built-in commands are core commands implemented directly in Lyra, with no external programs invoked. They are deeply integrated with Lyra's data model, providing a consistent interface and behavior.

## Command Categories

- [Filesystem](#filesystem) - cd, ls, pwd, mkdir, rm, cp, mv, touch
- [Text Processing](#text-processing) - cat, echo, head, tail, grep
- [Data Manipulation](#data-manipulation) - where, select, sort-by, group-by, join, take, skip
- [Aggregation Functions](#aggregation-functions) - count, sum, avg, min, max, stats
- [System Information](#system-information) - ps, env, which, uname
- [Conversion Commands](#conversion-commands) - from-json, to-json, from-csv, to-csv, from-yaml, to-yaml
- [Utility Commands](#utility-commands) - help, history, config, alias, exit

---

## Filesystem

### cd - Change Directory

**Syntax:**
```rust
cd [path]
```

**Parameters:**
- `path` - Target directory (optional, defaults to `~`)

**Examples:**
```rust
λ cd ~/projects
λ cd ..
λ cd -        # Go back to the previous directory
λ cd          # Go to the home directory
```

**Special paths:**
- `~` - Home directory
- `-` - Previous directory
- `.` - Current directory
- `..` - Parent directory

---

### ls - List Files

**Syntax:**
```rust
ls [path] [--all] [--long] [--recursive]
```

**Parameters:**
- `path` - Directory path (optional, defaults to the current directory)

**Options:**
- `--all` / `-a` - Show hidden files
- `--long` / `-l` - Detailed information
- `--recursive` / `-r` - Recursively list subdirectories

**Returns:** Table
```rust
╭───┬──────────┬──────┬─────────────────────╮
│ # │   name   │ size │      modified       │
├───┼──────────┼──────┼─────────────────────┤
│ 0 │ main.rs  │ 2.1K │ 2024-01-15 14:30:22 │
╰───┴──────────┴──────┴─────────────────────╯
```

**Examples:**
```rust
λ ls
λ ls /etc
λ ls --all
λ ls -la ~/projects
λ ls --recursive | where size > 1MB
```

---

### pwd - Print Current Directory

**Syntax:**
```rust
pwd
```

**Returns:** Path

**Examples:**
```rust
λ pwd
/home/user/projects/sol
```

---

### mkdir - Create Directory

**Syntax:**
```rust
mkdir <path> [--parents]
```

**Parameters:**
- `path` - Directory path

**Options:**
- `--parents` / `-p` - Create parent directories

**Examples:**
```rust
λ mkdir new-folder
λ mkdir -p deep/nested/folder
```

---

### rm - Remove Files or Directories

**Syntax:**
```rust
rm <paths...> [--recursive] [--force]
```

**Parameters:**
- `paths` - One or more paths

**Options:**
- `--recursive` / `-r` - Recursively remove directories
- `--force` / `-f` - Force removal without prompting

**Safety checks:**
- Removing multiple files requires confirmation
- Removing system directories is blocked

**Examples:**
```rust
λ rm file.txt
λ rm -r folder/
λ rm *.log        # Remove all .log files (asks for confirmation)
```

---

### cp - Copy Files

**Syntax:**
```rust
cp <source> <dest> [--recursive]
```

**Parameters:**
- `source` - Source path
- `dest` - Destination path

**Options:**
- `--recursive` / `-r` - Recursively copy directories

**Examples:**
```rust
λ cp file.txt backup.txt
λ cp -r folder/ backup-folder/
λ cp *.rs src/     # Copy all .rs files into src/
```

---

### mv - Move/Rename Files

**Syntax:**
```rust
mv <source> <dest>
```

**Parameters:**
- `source` - Source path
- `dest` - Destination path

**Examples:**
```rust
λ mv old.txt new.txt       # Rename
λ mv file.txt folder/       # Move
λ mv *.log archive/         # Move multiple files
```

---

### touch - Create Empty Files or Update Timestamps

**Syntax:**
```rust
touch <paths...>
```

**Parameters:**
- `paths` - One or more file paths

**Examples:**
```rust
λ touch new-file.txt
λ touch file1.txt file2.txt file3.txt
```

---

## Text Processing

### cat - Display File Contents

**Syntax:**
```rust
cat <paths...>
```

**Parameters:**
- `paths` - One or more file paths

**Returns:** String (multiple files are concatenated)

**Examples:**
```rust
λ cat README.md
λ cat file1.txt file2.txt
λ cat *.log | grep ERROR
```

---

### echo - Output Text

**Syntax:**
```rust
echo <values...>
```

**Parameters:**
- `values` - Values to output

**Examples:**
```rust
λ echo "Hello, World!"
λ echo $name
λ echo "Count:" $count
```

---

### head - Show the Beginning of a File

**Syntax:**
```rust
head [path] [--lines <n>]
```

**Parameters:**
- `path` - File path (optional; reads from the pipeline otherwise)

**Options:**
- `--lines <n>` / `-n <n>` - Number of lines (default 10)

**Examples:**
```rust
λ head file.txt
λ head -n 20 file.txt
λ cat large.log | head -n 5
```

---

### tail - Show the End of a File

**Syntax:**
```rust
tail [path] [--lines <n>] [--follow]
```

**Parameters:**
- `path` - File path (optional; reads from the pipeline otherwise)

**Options:**
- `--lines <n>` / `-n <n>` - Number of lines (default 10)
- `--follow` / `-f` - Keep watching the file for updates

**Examples:**
```rust
λ tail file.txt
λ tail -n 20 file.txt
λ tail -f /var/log/syslog    # Watch a log in real time
```

---

### grep - Search Text

**Syntax:**
```rust
grep <pattern> [paths...] [--ignore-case]
```

**Parameters:**
- `pattern` - Search pattern (regular expression)
- `paths` - File paths (optional; reads from the pipeline otherwise)

**Options:**
- `--ignore-case` / `-i` - Ignore case

**Returns:** Table (containing file, line, text)

**Examples:**
```rust
λ grep "TODO" src/*.rs
λ cat file.txt | grep "error"
λ grep -i "warning" *.log
```

---

## Data Manipulation

### where - Filter Rows

**Syntax:**
```rust
where <condition>
```

**Parameters:**
- `condition` - Filter condition

**Examples:**
```rust
λ ls | where size > 1MB
λ ps | where cpu > 50
λ ls | where name =~ "\.rs$"
λ users | where age >= 18 and role == "admin"
```

**Supported operators:**
- `==`, `!=` - Equal / not equal
- `>`, `<`, `>=`, `<=` - Comparison
- `=~`, `!~` - Regex match / non-match
- `in`, `not-in` - Contains / does not contain
- `and`, `or`, `not` - Logical operations

---

### select - Select Columns

**Syntax:**
```rust
select <columns...>
```

**Parameters:**
- `columns` - List of column names; renaming is allowed

**Examples:**
```rust
λ ps | select name pid cpu
λ ls | select name size
λ users | select name email role
λ data | select id title="name" content="body"  # Rename columns
```

---

### sort-by - Sort

**Syntax:**
```rust
sort-by <column> [--reverse]
```

**Parameters:**
- `column` - Column name to sort by

**Options:**
- `--reverse` / `-r` - Descending order

**Examples:**
```rust
λ ls | sort-by size
λ ls | sort-by modified --reverse
λ ps | sort-by cpu -r | take 10
```

---

### group-by - Group

**Syntax:**
```rust
group-by <column>
```

**Parameters:**
- `column` - Column name to group by

**Returns:** Table (one row per group, containing the original column values and an items list)

**Examples:**
```rust
λ ps | group-by user
λ sales | group-by product | each {
│   {
│     product: ${it.product},
│     total: (${it.items} | sum amount)
│   }
│ }
```

---

### join - Join Tables

**Syntax:**
```rust
join <other> on <key1>=<key2> [--left] [--right] [--outer]
```

**Parameters:**
- `other` - The other table to join with
- `key1` - Key column of the left table
- `key2` - Key column of the right table

**Options:**
- `--left` - Left join
- `--right` - Right join
- `--outer` - Outer join

**Examples:**
```rust
λ let users = [{id: 1, name: "Alice"}]
λ let orders = [{user_id: 1, item: "Book"}]
λ $users | join $orders on id=user_id
```

---

### take - Take the First N

**Syntax:**
```rust
take <n>
```

**Parameters:**
- `n` - Number of rows to take

**Examples:**
```rust
λ ls | take 10
λ ps | sort-by cpu -r | take 5
```

---

### skip - Skip the First N

**Syntax:**
```rust
skip <n>
```

**Parameters:**
- `n` - Number of rows to skip

**Examples:**
```rust
λ ls | skip 5
λ ls | skip 10 | take 10    # Second page (10 items per page)
```

---

### unique - Deduplicate

**Syntax:**
```rust
unique [column]
```

**Parameters:**
- `column` - Deduplicate by this column (optional)

**Examples:**
```rust
λ [1, 2, 2, 3] | unique
λ users | unique email
```

---

### reverse - Reverse Order

**Syntax:**
```rust
reverse
```

**Examples:**
```rust
λ [1, 2, 3] | reverse
[3, 2, 1]

λ ls | reverse
```

---

## Aggregation Functions

### count - Count

**Syntax:**
```rust
count
```

**Returns:** Number

**Examples:**
```rust
λ ls | count
42

λ ps | where user == "root" | count
15
```

---

### sum - Sum

**Syntax:**
```rust
sum [column]
```

**Parameters:**
- `column` - Column name (required for tables)

**Examples:**
```rust
λ [1, 2, 3, 4, 5] | sum
15

λ sales | sum amount
15420.50
```

---

### avg - Average

**Syntax:**
```rust
avg [column]
```

**Parameters:**
- `column` - Column name (required for tables)

**Examples:**
```rust
λ [1, 2, 3, 4, 5] | avg
3

λ sales | avg amount
308.41
```

---

### min / max - Minimum/Maximum

**Syntax:**
```rust
min [column]
max [column]
```

**Parameters:**
- `column` - Column name (required for tables)

**Examples:**
```rust
λ [5, 2, 8, 1, 9] | min
1

λ temperatures | max value
32.8
```

---

### stats - Statistics

**Syntax:**
```rust
stats [column]
```

**Parameters:**
- `column` - Column name (required for tables)

**Returns:** Record (count, sum, mean, median, min, max, stddev)

**Examples:**
```rust
λ [1, 2, 3, 4, 5] | stats
{
  count: 5,
  sum: 15,
  mean: 3,
  median: 3,
  min: 1,
  max: 5,
  stddev: 1.41
}
```

---

## System Information

### ps - Process List

**Syntax:**
```rust
ps [--all]
```

**Options:**
- `--all` / `-a` - Show processes for all users

**Returns:** Table
```rust
╭───┬──────────┬──────┬──────┬────────╮
│ # │   name   │ pid  │ cpu  │ memory │
├───┼──────────┼──────┼──────┼────────┤
│ 0 │ firefox  │ 1234 │ 12.5 │  1.2GB │
╰───┴──────────┴──────┴──────┴────────╯
```

**Examples:**
```rust
λ ps
λ ps --all
λ ps | where cpu > 50
λ ps | sort-by memory -r | take 5
```

---

### env - Environment Variables

**Syntax:**
```rust
env [name] [value]
```

**Parameters:**
- `name` - Variable name (optional; lists all when omitted)
- `value` - Variable value (used when setting)

**Examples:**
```rust
λ env                    # List all environment variables
λ env PATH               # View a specific variable
λ env EDITOR helix       # Set a variable
```

---

### which - Locate a Command

**Syntax:**
```rust
which <command>
```

**Parameters:**
- `command` - Command name

**Returns:** Path

**Examples:**
```rust
λ which git
/usr/bin/git

λ which ls
builtin (Lyra builtin command)
```

---

### uname - System Information

**Syntax:**
```rust
uname [--all]
```

**Options:**
- `--all` / `-a` - Show all information

**Returns:** Record

**Examples:**
```rust
λ uname
{
  os: "Linux",
  kernel: "6.5.0",
  hostname: "sol-dev",
  arch: "x86_64"
}
```

---

## Conversion Commands

### from-json - Parse JSON

**Syntax:**
```rust
from-json
```

**Input:** String (JSON text)
**Output:** The corresponding Lyra value

**Examples:**
```rust
λ cat data.json | from-json
λ echo '{"name": "Alice", "age": 30}' | from-json
{name: "Alice", age: 30}
```

---

### to-json - Convert to JSON

**Syntax:**
```rust
to-json [--pretty]
```

**Options:**
- `--pretty` / `-p` - Formatted output

**Examples:**
```rust
λ ls | to-json
λ {name: "Alice", age: 30} | to-json --pretty
{
  "name": "Alice",
  "age": 30
}
```

---

### from-csv - Parse CSV

**Syntax:**
```rust
from-csv [--no-header]
```

**Options:**
- `--no-header` - The CSV has no header row

**Input:** String (CSV text)
**Output:** Table

**Examples:**
```rust
λ cat users.csv | from-csv
╭───┬───────┬─────┬────────────────╮
│ # │ name  │ age │     email      │
├───┼───────┼─────┼────────────────┤
│ 0 │ Alice │  30 │ alice@ex.com   │
╰───┴───────┴─────┴────────────────╯
```

---

### to-csv - Convert to CSV

**Syntax:**
```rust
to-csv [--no-header]
```

**Options:**
- `--no-header` - Do not output a header row

**Examples:**
```rust
λ ls | to-csv
λ users | to-csv --no-header > data.csv
```

---

### from-yaml - Parse YAML

**Syntax:**
```rust
from-yaml
```

**Input:** String (YAML text)
**Output:** The corresponding Lyra value

**Examples:**
```rust
λ cat config.yaml | from-yaml
```

---

### to-yaml - Convert to YAML

**Syntax:**
```rust
to-yaml
```

**Examples:**
```rust
λ config | to-yaml
λ ls | to-yaml > files.yaml
```

---

## Utility Commands

### help - Help Information

**Syntax:**
```rust
help [command]
```

**Parameters:**
- `command` - Command name (optional)

**Examples:**
```rust
λ help              # Show all commands
λ help ls           # Show help for ls
λ help where        # Show help for where
```

---

### history - Command History

**Syntax:**
```rust
history [--clear] [--search <pattern>]
```

**Options:**
- `--clear` - Clear history
- `--search <pattern>` / `-s <pattern>` - Search history

**Examples:**
```rust
λ history                    # Show history
λ history --search "git"     # Search for commands containing "git"
λ history --clear            # Clear history
```

---

### config - Configuration Management

**Syntax:**
```rust
config get [key]
config set <key> <value>
config reset [key]
config list
```

**Subcommands:**
- `get [key]` - Get configuration (shows everything when key is omitted)
- `set <key> <value>` - Set configuration
- `reset [key]` - Reset to defaults
- `list` - List all configuration entries

**Examples:**
```rust
λ config get prompt.symbol
λ

λ config set prompt.symbol "❯"
λ config set prompt.colors.symbol "#38BDF8"
λ config reset prompt
λ config list
```

---

### alias - Alias Management

**Syntax:**
```rust
alias [name] [command]
alias list
alias remove <name>
```

**Parameters:**
- `name` - Alias name
- `command` - The corresponding command

**Subcommands:**
- `list` - List all aliases
- `remove <name>` - Remove an alias

**Examples:**
```rust
λ alias ll "ls -l"
λ alias gst "git status"
λ alias list
λ alias remove ll
```

---

### exit - Exit the Shell

**Syntax:**
```rust
exit [code]
```

**Parameters:**
- `code` - Exit code (optional, defaults to 0)

**Examples:**
```rust
λ exit
λ exit 1    # Exit with an error code
```

---

## Pipeline Composition Examples

### File Management

```rust
# Find the 10 largest files
λ ls --recursive | sort-by size -r | take 10

# Clean up temporary files
λ ls /tmp | where modified < (date now - 7d) | each { rm $it.name }

# Back up important files
λ ls | where name =~ "\.(rs|toml|md)$" | each { cp $it.name "backup/${it.name}" }
```

### System Administration

```rust
# Find the processes using the most memory
λ ps | sort-by memory -r | take 5 | select name pid memory

# Monitor CPU usage
λ while true {
│   ps | where cpu > 80 | each { echo "High CPU: ${it.name} (${it.cpu}%)" }
│   sleep 5s
│ }
```

### Data Analysis

```rust
# Analyze logs
λ cat access.log \
│   | from-csv \
│   | where status >= 400 \
│   | group-by status \
│   | each { {status: ${it.status}, count: (${it.items} | count)} }

# Count lines of code
λ ls --recursive \
│   | where name =~ "\.rs$" \
│   | each { {file: ${it.name}, lines: (cat ${it.name} | count)} } \
│   | sort-by lines -r
```

## Extensibility

All built-in commands implement the unified `Builtin` trait, making it easy to add new commands or extend functionality through the plugin system.

See also:
- [Plugin System](./plugins.md) - How to write custom commands
- [Architecture](./architecture.md) - Implementation details of built-in commands
