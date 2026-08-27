# Lyra Data Model

Lyra's core innovation is the structured data pipeline. Unlike the text streams of traditional shells, Lyra's pipelines carry typed data structures, making command composition more powerful and safer.

## Core Idea

### The Problem with Traditional Shells

```bash
# Bash: text stream processing
$ ps aux | grep python | awk '{print $2}' | xargs kill

# Problems:
# 1. Every command has to re-parse text
# 2. Column positions are implicit (what is column 2?)
# 3. Spaces and special characters require complex escaping
# 4. Errors are easy to miss (did some command in the pipeline fail?)
```

### Lyra's Approach

```rust
# Lyra: structured data flow
λ ps | where name =~ "python" | select pid | each { |p| kill $p.pid }

# Advantages:
# 1. Data keeps its structure (no parsing needed)
# 2. Column names are explicit (.pid is clearly readable)
# 3. Type safety (no accidentally treating a string as a number)
# 4. Built-in error handling (errors propagate through the pipeline)
```

## Value Type System

### Basic Types

#### String

```rust
λ let name = "Alice"
λ echo $name
Alice

λ let message = "Hello, ${name}!"
λ echo $message
Hello, Alice!

# Multi-line string
λ let poem = """
│ Roses are red
│ Violets are blue
│ """
```

**Methods:**
```rust
$string | length          # String length
$string | to-uppercase    # Convert to uppercase
$string | to-lowercase    # Convert to lowercase
$string | trim            # Trim leading/trailing whitespace
$string | split ","       # Split
$string | replace "old" "new"  # Replace
```

#### Number

```rust
λ let count = 42
λ let price = 19.99
λ let sci = 1.23e-4

# Common math operations are supported
λ echo ${10 + 5}
15

λ echo ${2 ** 8}
256
```

**Methods:**
```rust
$number | abs             # Absolute value
$number | round           # Round
$number | floor           # Round down
$number | ceil            # Round up
$number | sqrt            # Square root
```

#### Bool

```rust
λ let is_valid = true
λ let is_empty = false

# Logical operations
λ if $is_valid and not $is_empty {
│   echo "Valid and not empty"
│ }
```

#### Null

```rust
λ let nothing = null

λ if $value == null {
│   echo "No value"
│ }
```

### Collection Types

#### List

```rust
# Create lists
λ let numbers = [1, 2, 3, 4, 5]
λ let colors = ["red", "green", "blue"]
λ let mixed = [1, "two", true, null]

# Index access (starting from 0)
λ echo ${numbers[0]}
1

λ echo ${colors[2]}
blue

# Slices
λ echo ${numbers[1..4]}
[2, 3, 4]

λ echo ${numbers[..3]}
[1, 2, 3]

λ echo ${numbers[2..]}
[3, 4, 5]

# Negative indices (from the end)
λ echo ${numbers[-1]}
5

λ echo ${numbers[-3..-1]}
[3, 4, 5]
```

**Methods:**
```rust
$list | length            # List length
$list | first             # First element
$list | last              # Last element
$list | append $item      # Append an element
$list | prepend $item     # Prepend an element
$list | reverse           # Reverse
$list | unique            # Deduplicate
$list | flatten           # Flatten nested lists
$list | sort              # Sort
$list | sort-by $field    # Sort by field
```

**Iteration:**
```rust
# each: apply a function to every element
λ [1, 2, 3] | each { |n| echo ${n * 2} }
2
4
6

# map: transform every element
λ let doubled = ([1, 2, 3] | map { |n| $n * 2 })
λ echo $doubled
[2, 4, 6]

# filter/where: filter elements
λ [1, 2, 3, 4, 5] | where $it > 2
[3, 4, 5]

# reduce: aggregate
λ [1, 2, 3, 4, 5] | reduce { |acc, n| $acc + $n }
15
```

#### Record

Similar to JSON objects or Python dictionaries:

```rust
# Create a record
λ let person = {
│   name: "Alice",
│   age: 30,
│   email: "alice@example.com"
│ }

# Access fields
λ echo ${person.name}
Alice

λ echo ${person.age}
30

# Dynamic access
λ let field = "email"
λ echo ${person[$field]}
alice@example.com

# Nested records
λ let user = {
│   profile: {
│     name: "Bob",
│     location: "NYC"
│   },
│   settings: {
│     theme: "dark",
│     notifications: true
│   }
│ }

λ echo ${user.profile.name}
Bob

λ echo ${user.settings.theme}
dark
```

**Methods:**
```rust
$record | keys            # Get all keys
$record | values          # Get all values
$record | has-key "name"  # Check whether a key exists
$record | merge $other    # Merge records
$record | select name age # Select specific fields
```

### Special Types

#### Table

The most important data structure in pipelines:

```rust
# ls returns a table
λ ls
╭───┬──────────┬──────┬─────────────────────╮
│ # │   name   │ size │      modified       │
├───┼──────────┼──────┼─────────────────────┤
│ 0 │ main.rs  │ 2.1K │ 2024-01-15 14:30:22 │
│ 1 │ lib.rs   │ 1.5K │ 2024-01-15 09:15:03 │
╰───┴──────────┴──────┴─────────────────────╯

# A table is a list of records
λ ls | first
{name: "main.rs", size: 2100, modified: "2024-01-15 14:30:22"}

# Access columns
λ ls | select name size
╭───┬──────────┬──────╮
│ # │   name   │ size │
├───┼──────────┼──────┤
│ 0 │ main.rs  │ 2.1K │
│ 1 │ lib.rs   │ 1.5K │
╰───┴──────────┴──────╯

# Filter rows
λ ls | where size > 1KB
╭───┬──────────┬──────┬─────────────────────╮
│ # │   name   │ size │      modified       │
├───┼──────────┼──────┼─────────────────────┤
│ 0 │ main.rs  │ 2.1K │ 2024-01-15 14:30:22 │
│ 1 │ lib.rs   │ 1.5K │ 2024-01-15 09:15:03 │
╰───┴──────────┴──────┴─────────────────────╯
```

**Operations:**
```rust
# Select columns
$table | select col1 col2

# Filter rows
$table | where condition

# Sort
$table | sort-by column
$table | sort-by column --reverse

# Aggregate
$table | count
$table | sum column
$table | avg column
$table | min column
$table | max column

# Group
$table | group-by column

# Join
$table1 | join $table2 on column
```

#### Path

A dedicated path type for handling filesystem paths:

```rust
λ let home = ~/
λ let project = ~/projects/sol

# Path operations
λ echo ${project | path-exists}
true

λ echo ${project | path-basename}
sol

λ echo ${project | path-dirname}
/home/user/projects

λ echo ${project | path-extension}
(none)

# Path composition
λ let config = (~/config | path-join "lyra" "config.toml")
λ echo $config
/home/user/config/lyra/config.toml
```

#### Duration

```rust
λ let timeout = 5s
λ let interval = 100ms
λ let delay = 2.5h

# Duration arithmetic
λ echo ${5s + 500ms}
5.5s

λ echo ${1h - 15min}
45min

# Duration comparison
λ if $elapsed > 1s {
│   echo "Slow command"
│ }
```

**Units:**
- `ns` - Nanoseconds
- `us` / `µs` - Microseconds
- `ms` - Milliseconds
- `s` - Seconds
- `min` - Minutes
- `h` - Hours
- `d` - Days

#### DateTime

```rust
λ let now = (date now)
λ echo $now
2024-01-15 14:30:22 UTC

# Date arithmetic
λ let tomorrow = (${now} + 1d)
λ let last_week = (${now} - 7d)

# Formatting
λ echo (${now} | date-format "%Y-%m-%d")
2024-01-15

λ echo (${now} | date-format "%H:%M:%S")
14:30:22

# Parsing
λ let parsed = (date-parse "2024-01-15" "%Y-%m-%d")
```

## Type Conversion

### Automatic Conversion

In some cases, Lyra converts types automatically:

```rust
# String + number → string concatenation
λ echo ${"Count: " + 42}
Count: 42

# Number + string → attempts to parse
λ echo ${42 + "8"}
50

# Booleans are used automatically in conditions
λ if "hello" {  # Non-empty string → true
│   echo "String is truthy"
│ }
```

### Explicit Conversion

Use the `to-*` and `from-*` commands:

```rust
# Convert to string
λ 42 | to-string
"42"

# Convert to number
λ "123" | to-number
123

# Convert to bool
λ "true" | to-bool
true

# Convert to list
λ "a,b,c" | split "," | to-list
["a", "b", "c"]

# Format conversion
λ cat data.json | from-json
{...}

λ ls | to-json
[{"name": "...", ...}, ...]

λ cat users.csv | from-csv
╭───┬──────┬─────┬────────────────╮
│ # │ name │ age │     email      │
├───┼──────┼─────┼────────────────┤
│ 0 │ Alice│  30 │ alice@ex.com   │
╰───┴──────┴─────┴────────────────╯

λ ps | to-csv > processes.csv
```

## Pipeline Semantics

### Pipeline Variables

Inside a pipeline, the special variables `$in` and `$it` reference the current value:

```rust
# $in: pipeline input
λ def double [] {
│   let input = $in
│   return ${input * 2}
│ }

λ 21 | double
42

# $it: the current element during iteration
λ [1, 2, 3] | each { |x| echo ${x * 2} }
2
4
6

# Or use $it (if no named parameter is given)
λ [1, 2, 3] | each { echo ${it * 2} }
2
4
6
```

### Pipeline Error Handling

Errors propagate through pipelines:

```rust
λ cat missing.txt | from-json | select name
Error: File not found: missing.txt
# Subsequent commands do not run

# Use try to catch errors
λ try {
│   cat missing.txt | from-json
│ } catch {
│   echo "Failed: ${error.message}"
│   return {default: "data"}
│ }
```

### Pipeline Type Checking

Lyra performs type checking within pipelines:

```rust
# Type mismatch
λ "hello" | sort-by name
Error: sort-by expects a table, got string

# Correct types
λ ls | sort-by name
# ✓ Success

# Smart conversion
λ [3, 1, 2] | sort
[1, 2, 3]
# The list is converted to a table, sorted, and converted back to a list
```

## Data Manipulation Commands

### where (Filter)

```rust
# Simple condition
λ ls | where size > 1MB

# Complex condition
λ ps | where cpu > 50 and memory > 1GB

# Regex matching
λ ls | where name =~ "\.rs$"

# List membership
λ users | where role in ["admin", "moderator"]

# Null checks
λ data | where field != null
```

### select (Select Columns)

```rust
# Select specific columns
λ ps | select name pid cpu

# Rename columns
λ ps | select name process_id=pid

# Computed columns
λ ls | select name size_mb={size / 1MB}
```

### sort-by (Sort)

```rust
# Ascending
λ ls | sort-by size

# Descending
λ ls | sort-by size --reverse

# Multi-column sort
λ users | sort-by role name
```

### group-by (Group)

```rust
# Group by column
λ ps | group-by user
╭──────┬────────────────────────╮
│ user │        processes        │
├──────┼────────────────────────┤
│ root │ [sshd, systemd, ...]   │
│ alice│ [firefox, code, ...]   │
╰──────┴────────────────────────╯

# Grouped aggregation
λ sales | group-by product | each {
│   {
│     product: ${it.product},
│     total: (${it.sales} | sum)
│   }
│ }
```

### join (Join)

```rust
# Inner join
λ let users = [{id: 1, name: "Alice"}, {id: 2, name: "Bob"}]
λ let orders = [{user_id: 1, item: "Book"}, {user_id: 1, item: "Pen"}]
λ $users | join $orders on id=user_id
╭───┬────┬───────┬──────────┬──────╮
│ # │ id │ name  │ user_id  │ item │
├───┼────┼───────┼──────────┼──────┤
│ 0 │  1 │ Alice │    1     │ Book │
│ 1 │  1 │ Alice │    1     │ Pen  │
╰───┴────┴───────┴──────────┴──────╯

# Left join
λ $users | join $orders on id=user_id --left

# Outer join
λ $users | join $orders on id=user_id --outer
```

### take / skip (Paging)

```rust
# Take the first N
λ ls | take 10

# Skip the first N
λ ls | skip 5

# Paging
λ ls | skip 10 | take 10  # Second page
```

### unique (Deduplicate)

```rust
# Simple deduplication
λ [1, 2, 2, 3, 3, 3] | unique
[1, 2, 3]

# Deduplicate by column
λ users | unique name
```

### flatten (Flatten)

```rust
# Flatten nested lists
λ [[1, 2], [3, 4], [5]] | flatten
[1, 2, 3, 4, 5]

# Flatten nested records
λ {a: {b: {c: 1}}} | flatten
{a.b.c: 1}
```

## Aggregation Functions

```rust
# Count
λ ls | count
42

# Sum
λ sales | sum amount
15420.50

# Average
λ sales | avg amount
308.41

# Min/max
λ temperatures | min value
-5.2

λ temperatures | max value
32.8

# Statistics
λ data | stats column
{
  count: 100,
  sum: 5050,
  mean: 50.5,
  median: 50.5,
  min: 1,
  max: 100,
  stddev: 29.01
}
```

## Practical Examples

### 1. System Monitoring

```rust
# Find the processes using the most CPU
λ ps | sort-by cpu --reverse | take 5 | select name cpu memory

# Monitor disk usage
λ df | where use_percent > 80 | select filesystem use_percent

# View network connections
λ netstat | where state == "ESTABLISHED" | group-by remote_host | each {
│   {host: ${it.remote_host}, count: (${it.connections} | count)}
│ }
```

### 2. Log Analysis

```rust
# Analyze access logs
λ cat access.log \
│   | from-csv \
│   | where status >= 400 \
│   | group-by status \
│   | each { {status: ${it.status}, count: (${it.entries} | count)} } \
│   | sort-by count --reverse

# Find the most frequently accessed paths
λ cat access.log \
│   | from-csv \
│   | group-by path \
│   | each { {path: ${it.path}, hits: (${it.entries} | count)} } \
│   | sort-by hits --reverse \
│   | take 10
```

### 3. Git Repository Analysis

```rust
# Count commits per author
λ git log --format="%an" \
│   | split "\n" \
│   | group-by \
│   | each { {author: ${it.value}, commits: (${it.items} | count)} } \
│   | sort-by commits --reverse

# Find the largest files
λ git ls-files \
│   | each { {path: $it, size: (ls $it | first | select size)} } \
│   | sort-by size --reverse \
│   | take 10
```

### 4. Data Transformation

```rust
# JSON to CSV
λ cat users.json | from-json | to-csv > users.csv

# CSV to JSON (with filtering)
λ cat users.csv \
│   | from-csv \
│   | where age > 18 \
│   | to-json > adults.json

# Merge multiple data sources
λ let json_data = (cat data.json | from-json)
λ let csv_data = (cat data.csv | from-csv)
λ $json_data | append $csv_data | to-yaml > combined.yaml
```

## Performance Considerations

### Lazy Evaluation

Lyra uses lazy evaluation to optimize pipeline performance:

```rust
# Only the first 10 results are needed; not every file is processed
λ ls --recursive / | where size > 1GB | take 10
# Stops as soon as 10 are found
```

### Streaming

Large datasets are processed as streams without loading everything into memory:

```rust
# Process a multi-GB log file
λ cat huge.log | from-csv | where error == true | count
# Line-by-line processing with constant memory usage
```

### Parallel Processing

Parallel processing is supported (a Phase 3 feature):

```rust
# Process each file in parallel
λ ls *.txt | each --parallel { |file|
│   cat $file | from-json | validate-schema
│ }
```

## Type System Summary

Lyra's type system delivers:

1. **Richness** - Supports everything from simple values to complex tables
2. **Safety** - Compile-time and runtime type checking
3. **Interoperability** - Easy conversion to and from external data formats (JSON/CSV/YAML)
4. **Pipeline friendliness** - Every type can flow through a pipeline
5. **Performance** - Lazy evaluation and streaming avoid unnecessary overhead

This makes Lyra suitable both for simple file operations and for complex data analysis tasks.
