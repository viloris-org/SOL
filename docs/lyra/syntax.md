# Lyra Syntax Design

Lyra's syntax design pursues consistency, readability, and expressiveness. Unlike traditional shells (bash/zsh) with their historical baggage, Lyra starts from scratch and establishes unified syntax rules.

## Design Principles

1. **Uniformity** - The same concept is expressed with the same syntax
2. **Predictability** - Syntax rules have no special cases or context-dependent ambiguity
3. **Human-friendly** - A command style close to natural language
4. **Type safety** - Variables and expressions have explicit types
5. **Gradual complexity** - Simple tasks are simple to write; complex tasks remain expressive

## Basic Syntax

### Command Invocation

The simplest form: a command name followed by arguments

```rust
λ echo hello world
hello world

λ ls -l
# List the current directory

λ cd /home/user/projects
# Change directory
```

**Rules:**
- The command name is the first word
- Arguments are separated by spaces
- Simple words do not need quotes

### Variables

Variables use the `$` prefix; assignment uses `let`

```rust
λ let name = "SOL"
λ echo $name
SOL

λ let count = 42
λ echo $count
42

λ let items = [1, 2, 3, 4, 5]
λ echo $items
[1, 2, 3, 4, 5]
```

**Rules:**
- Simple variables: `$name`
- Expressions: `${name.length}` `${items[0]}` `${count + 1}`
- Scope: local variables are valid within the current shell session

### Strings

Four string forms:

```rust
# 1. Bare string - simple words need no quotes
λ echo hello
hello

# 2. Double-quoted string - supports interpolation
λ let name = "World"
λ echo "Hello, $name!"
Hello, World!

# 3. Single-quoted string - literal, no interpolation
λ echo 'Hello, $name!'
Hello, $name!

# 4. Multi-line string
λ let poem = """
│ Roses are red
│ Violets are blue
│ Lyra is elegant
│ And functional too
│ """
```

**Interpolation syntax:**
```rust
λ let x = 10
λ echo "Value: $x"              # Simple variable
Value: 10

λ echo "Double: ${x * 2}"       # Expression
Double: 20

λ echo "Items: ${[1, 2, 3]}"    # Complex expression
Items: [1, 2, 3]
```

### Lists

```rust
λ let colors = ["red", "green", "blue"]
λ echo $colors
[red, green, blue]

λ echo ${colors[0]}
red

λ echo ${colors | length}
3

# List comprehension
λ let squares = [for x in 1..5 { $x * $x }]
λ echo $squares
[1, 4, 9, 16, 25]
```

### Records

Similar to JSON objects:

```rust
λ let person = {
│   name: "Alice",
│   age: 30,
│   role: "developer"
│ }

λ echo ${person.name}
Alice

λ echo ${person.age}
30

# Nested structures
λ let project = {
│   name: "SOL",
│   version: "0.1.0",
│   maintainer: {
│     name: "rownix",
│     email: "rownix@example.com"
│   }
│ }

λ echo ${project.maintainer.name}
rownix
```

## Pipelines and Data Flow

Lyra's core feature: structured data pipelines

### Basic Pipelines

```rust
# Traditional: text stream
λ ls | grep ".rs"

# Lyra: structured data stream
λ ls | where name =~ ".rs$"
╭───┬──────────────┬──────┬─────────────────────╮
│ # │     name     │ size │      modified       │
├───┼──────────────┼──────┼─────────────────────┤
│ 0 │ main.rs      │ 2.1K │ 2024-01-15 14:30:22 │
│ 1 │ parser.rs    │ 8.4K │ 2024-01-15 09:15:03 │
│ 2 │ runtime.rs   │ 5.2K │ 2024-01-14 16:45:10 │
╰───┴──────────────┴──────┴─────────────────────╯
```

### Chained Operations

```rust
# Find large files, sort by size, take the top 10
λ ls --recursive \
│   | where size > 1MB \
│   | sort-by size --reverse \
│   | take 10

# Git commit history analysis
λ git log --oneline \
│   | parse "{hash} {message}" \
│   | where message =~ "feat:" \
│   | count

# Process management
λ ps \
│   | where cpu > 50 \
│   | sort-by memory --reverse \
│   | select name pid cpu memory
```

### Data Transformation

```rust
# JSON processing
λ cat data.json \
│   | from json \
│   | where .status == "active" \
│   | select name email \
│   | to csv

# CSV to table
λ cat users.csv \
│   | from csv \
│   | where age > 25 \
│   | sort-by age

# Multiple format support
λ ls | to json          # Convert to JSON
λ ls | to csv           # Convert to CSV
λ ls | to yaml          # Convert to YAML
λ ls | to table         # Convert to table (default)
```

## Conditionals and Control Flow

### If Expressions

```rust
# Single line
λ if $count > 10 { echo "Large" } else { echo "Small" }

# Multi-line
λ if $error {
│   echo "Command failed!"
│   exit 1
│ } else {
│   echo "Success"
│ }

# Else-if chain
λ if $status == 0 {
│   echo "Success"
│ } else if $status == 1 {
│   echo "Warning"
│ } else {
│   echo "Error"
│ }
```

### Pattern Matching

```rust
λ match $env {
│   "dev" => { echo "Development mode" },
│   "staging" => { echo "Staging mode" },
│   "prod" => { echo "Production mode" },
│   _ => { echo "Unknown environment" }
│ }
```

### Loops

```rust
# For loop
λ for file in (ls) {
│   echo "Processing: ${file.name}"
│ }

# While loop
λ let count = 0
λ while $count < 5 {
│   echo $count
│   let count = ${count + 1}
│ }

# Iterator style (recommended)
λ 1..10 | each { |n| echo $n }
λ ls | each { |file| cp $file.name "backup/${file.name}" }
```

## Function Definitions

### Basic Functions

```rust
λ def greet [name: string] {
│   echo "Hello, $name!"
│ }

λ greet Alice
Hello, Alice!
```

### Parameter Types and Default Values

```rust
λ def deploy [
│   env: string,              # Required parameter
│   --force: bool = false,    # Optional flag
│   --tag: string = "latest"  # Optional parameter with default value
│ ] {
│   if $env not-in ["dev", "staging", "prod"] {
│     error "Invalid environment: $env"
│   }
│   
│   echo "Deploying to $env with tag $tag"
│   
│   if $force {
│     echo "Force mode enabled"
│   }
│ }

# Invocation
λ deploy dev
λ deploy prod --force
λ deploy staging --tag v1.2.3
```

### Return Values

```rust
λ def double [x: int] {
│   return ${x * 2}
│ }

λ let result = (double 21)
λ echo $result
42

# Usage in pipelines
λ 1..5 | each { |n| double $n }
[2, 4, 6, 8, 10]
```

### Pipeline Functions

Special syntax for receiving pipeline input:

```rust
λ def filter-large [] {
│   where size > 1MB
│ }

λ ls | filter-large

# Use $in to access pipeline input
λ def summarize [] {
│   let total = ($in | length)
│   let sum = ($in | math sum)
│   {total: $total, sum: $sum}
│ }

λ [1, 2, 3, 4, 5] | summarize
{total: 5, sum: 15}
```

## Error Handling

### Error Catching

```rust
# Check whether the previous command failed
λ git pull
λ if $error {
│   echo "Pull failed: ${error.message}"
│   git stash && git pull && git stash pop
│ }

# Try-catch style
λ try {
│   risky-command
│ } catch {
│   echo "Command failed: ${error.message}"
│   echo "Exit code: ${error.code}"
│ }
```

### Error Propagation

```rust
λ def safe-deploy [env: string] {
│   # Use the ? operator to propagate errors
│   validate-env $env?
│   build-project?
│   run-tests?
│   deploy-to $env?
│   
│   echo "Deployment successful!"
│ }
```

## Operators

### Comparison Operators

```rust
==    # Equal
!=    # Not equal
>     # Greater than
<     # Less than
>=    # Greater than or equal
<=    # Less than or equal
=~    # Regex match
!~    # Regex non-match
in    # Contained in
not-in # Not contained in
```

Examples:
```rust
λ if $count > 10 { echo "Large" }
λ if $name == "Alice" { echo "Hi Alice" }
λ if $file =~ ".rs$" { echo "Rust file" }
λ if $env in ["dev", "staging"] { echo "Non-prod" }
```

### Logical Operators

```rust
and   # Logical AND
or    # Logical OR
not   # Logical NOT
```

Examples:
```rust
λ if $age > 18 and $age < 65 { echo "Working age" }
λ if $env == "prod" or $env == "staging" { echo "Careful!" }
λ if not $debug { echo "Release mode" }
```

### Arithmetic Operators

```rust
+     # Addition
-     # Subtraction
*     # Multiplication
/     # Division
%     # Modulo
**    # Exponentiation
```

Examples:
```rust
λ echo ${10 + 5}
15

λ echo ${2 ** 8}
256

λ let count = ${count + 1}
```

## Comments

```rust
# Single-line comment
λ echo "Hello"  # End-of-line comment

## Documentation comment (for functions)
λ def greet [name: string] {
│   ## Greet the user
│   ## Parameters:
│   ##   name - the user's name
│   echo "Hello, $name!"
│ }
```

## Scopes and Modules

### Local Scope

```rust
λ let global = "I'm global"

λ do {
│   let local = "I'm local"
│   echo $global   # Accessible
│   echo $local    # Accessible
│ }

λ echo $global     # Accessible
λ echo $local      # Error: undefined
```

### Importing Modules

```rust
# Import the standard library
λ use std.path
λ use std.string

# Import a custom module
λ use ~/lyra/modules/git.ly

# Use an alias
λ use std.filesystem as fs

# Use module functionality
λ git.recent       # From the git.ly module
λ fs.copy-recursive src/ dest/
```

### Exporting Definitions

```rust
# In a module file
export def git-recent [] {
  git log --oneline -20 | parse "{hash} {message}"
}

export def git-branches [] {
  git branch | parse "{name}"
}
```

## Special Syntax

### Command Substitution

```rust
# Capture command output with parentheses
λ let files = (ls | where size > 1MB)
λ echo $files

λ let count = (git log --oneline | count)
λ echo "Total commits: $count"
```

### Background Jobs

```rust
# & runs a job in the background
λ long-running-task &
[Job 1] Started

λ jobs
╭───┬─────┬───────────────────┬─────────╮
│ # │ ID  │      Command      │ Status  │
├───┼─────┼───────────────────┼─────────┤
│ 0 │  1  │ long-running-task │ Running │
╰───┴─────┴───────────────────┴─────────╯

λ fg 1    # Bring to the foreground
λ bg 1    # Resume running in the background
```

### Input/Output Redirection

```rust
# Output redirection
λ echo "Hello" > output.txt       # Overwrite
λ echo "World" >> output.txt      # Append

# Input redirection
λ sort < unsorted.txt

# Error redirection
λ risky-command 2> error.log      # Errors only
λ command > out.log 2> err.log    # Separate stdout and stderr
λ command >& all.log               # Merge stdout and stderr
```

### Command Chains

```rust
# && - run the next command only if the previous one succeeded
λ make && make test && make install

# || - run the next command only if the previous one failed
λ command1 || command2

# ; - run sequentially (regardless of success or failure)
λ cd /tmp; ls; cd -
```

## Comparison with Traditional Shells

### Variable Syntax

| Operation | Bash/Zsh | Lyra |
|-----------|----------|------|
| Assignment | `name=value` | `let name = value` |
| Read | `$name` | `$name` |
| Array | `arr=(1 2 3)` | `let arr = [1, 2, 3]` |
| Length | `${#name}` | `${name \| length}` |
| Slice | `${arr[@]:1:2}` | `${arr[1..3]}` |

### Conditional Syntax

| Operation | Bash/Zsh | Lyra |
|-----------|----------|------|
| If | `if [ $x -gt 10 ]; then ... fi` | `if $x > 10 { ... }` |
| String comparison | `if [ "$a" = "$b" ]` | `if $a == $b` |
| File test | `if [ -f file ]` | `if (path exists file)` |

### Loop Syntax

| Operation | Bash/Zsh | Lyra |
|-----------|----------|------|
| For | `for i in 1 2 3; do ... done` | `for i in [1, 2, 3] { ... }` |
| While | `while [ $i -lt 10 ]; do ... done` | `while $i < 10 { ... }` |
| Iteration | `for f in *.txt; do ... done` | `ls "*.txt" \| each { \|f\| ... }` |

### Function Syntax

| Operation | Bash/Zsh | Lyra |
|-----------|----------|------|
| Definition | `func() { echo $1; }` | `def func [x] { echo $x }` |
| Invocation | `func arg` | `func arg` |
| Return value | `return 0` | `return $value` |

## Syntax Highlighting

Lyra's syntax design accounts for highlighting readability:

```rust
# Keywords: blue
let if else for while def export use

# Strings: green
"hello" 'world'

# Variables: cyan
$name ${expr}

# Operators: orange
+ - * / == != > < and or

# Function calls: yellow
echo ls where sort-by

# Comments: gray
# This is a comment
```

## Best Practices

### 1. Naming Conventions

```rust
# Variables: lowercase with underscores
let user_name = "Alice"
let total_count = 42

# Functions: lowercase with hyphens
def get-user-info [] { ... }
def validate-input [] { ... }

# Constants: uppercase with underscores (by convention)
let MAX_RETRIES = 3
let API_URL = "https://api.example.com"
```

### 2. Type Annotations

Add type annotations whenever possible to improve readability and error detection:

```rust
# Good
def add [x: int, y: int] -> int {
  return ${x + y}
}

# Also works, but the types are unclear
def add [x, y] {
  return ${x + y}
}
```

### 3. Pipeline Style

Prefer pipelines over nesting:

```rust
# Discouraged: nesting
let result = (take 10 (sort-by size (where size > 1MB (ls))))

# Recommended: pipeline
let result = (ls | where size > 1MB | sort-by size | take 10)
```

### 4. Error Handling

Handle errors explicitly; never ignore them:

```rust
# Discouraged
git pull

# Recommended
git pull
if $error {
  echo "Pull failed, trying to rebase..."
  git pull --rebase
}
```

### 5. Documentation Comments

Document complex functions:

```rust
def deploy [
  env: string,
  --tag: string = "latest"
] {
  ## Deploy the app to the specified environment
  ##
  ## Parameters:
  ##   env - target environment (dev/staging/prod)
  ##   --tag - Docker image tag (default: latest)
  ##
  ## Examples:
  ##   deploy dev
  ##   deploy prod --tag v1.2.3
  
  # Implementation...
}
```

## Next Steps

- [Intelligence Features](./intelligence.md) - Completion, error correction, preview
- [Data Model](./data-model.md) - Structured data in depth
- [Built-in Commands](./builtins.md) - Core command reference
- [Plugin System](./plugins.md) - Extending Lyra

## Summary

Lyra's syntax design achieves three core goals:

1. **Consistency** - Unified variable, string, and function syntax with no special cases
2. **Expressiveness** - Structured data, pipelines, and pattern matching let complex tasks be expressed concisely
3. **Readability** - Close to natural language with explicit types, easy to understand and maintain

This makes Lyra well suited both for interactive use and for writing complex automation scripts.
