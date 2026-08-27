# Lyra Prompt Design

The prompt is the first visual element of the user's interaction with the shell. Lyra's prompt design aims to be elegant, informative, and customizable.

## Default Prompt

### Standard Mode

```rust
λ ~/projects/sol (main*) 2.3s
```

**Components:**
- `λ` - The prompt symbol (lambda), meaning "ready to accept a command"
- `~/projects/sol` - Current working directory
- `(main*)` - Git branch and status
- `2.3s` - Execution time of the previous command (shown when it exceeds 1 second)

### Error State

```rust
λ! ~/projects/sol (main*) [exit: 1]
```

- `λ!` - Red, indicating the previous command failed
- `[exit: 1]` - Exit code (shown when nonzero)

### Root Mode

```rust
# ~/system/config
```

- `#` - The root user gets the traditional `#` symbol as a visual warning for dangerous operations
- The entire line uses a warning color (orange/red)

## Why λ (Lambda)

### Philosophical Meaning

1. **Functional thinking** - Lyra treats commands as functions and pipelines as function composition
   ```rust
   λ ls | where size > 1MB | sort-by modified
   #    ↑          ↑                ↑
   #    function   function (filter) function (sort)
   ```

2. **Mathematical elegance** - The lambda calculus is a cornerstone of computer science, reflecting Lyra's theoretical foundations

3. **Concise expression** - A single symbol, visually clean, never getting in the way of the command itself

### Practical Advantages

1. **Easy to spot** - In walls of terminal output, λ stands out clearly as the start of a command
2. **Cross-platform support** - Modern terminals all support Unicode; λ renders well
3. **Community resonance** - REPLs of functional languages like Haskell and Lisp use similar symbols
4. **Distinctiveness** - Sets Lyra apart from other shells and builds brand recognition

### Aligned with the Target Users

SOL's target users are developers and power users who:
- Are familiar with functional programming concepts
- Appreciate elegant design
- Seek a modern tool experience

## Symbol Semantics

Lyra uses different prompt symbols to convey system state:

| Symbol | Meaning            | Color | Scenario                                  |
|--------|--------------------|-------|-------------------------------------------|
| `λ`    | Ready              | Cyan  | Normal command; last execution succeeded  |
| `λ!`   | Error state        | Red   | The last command failed                   |
| `#`    | Root privileges    | Orange| Running as root                           |
| `λ*`   | Dirty working tree | Yellow| Git repository has uncommitted changes    |
| `λ↑`   | Background jobs    | Blue  | Background processes running              |
| `λ⏸`   | Suspended mode     | Gray  | Job control suspended state               |

## Customizing the Prompt

### Basic Configuration

```toml
# ~/.config/lyra/config.toml
[prompt]
# Primary prompt symbol
symbol = "λ"

# Error prompt symbol
symbol_error = "λ!"

# Root prompt symbol
symbol_root = "#"

# Continuation prompt (second line and beyond)
symbol_continuation = "│"

# Color theme
[prompt.colors]
symbol = "#38BDF8"           # Cyan
symbol_error = "#E9A568"     # Orange-red
symbol_root = "#FF6B6B"      # Red
directory = "#6EE7B7"        # Green
git_branch = "#A78BFA"       # Purple
git_dirty = "#FBBF24"        # Yellow
duration = "#94A3B8"         # Gray
```

### Preset Themes

Lyra ships several preset themes for quick switching:

#### Minimalist
```rust
λ
```
Shows only the prompt symbol with no other information. Ideal for users who prefer simplicity.

```toml
[prompt]
preset = "minimalist"
```

#### Traditional
```rust
$ ~/projects/sol
```
A Bash-like `$` prompt for users accustomed to traditional shells.

```toml
[prompt]
preset = "traditional"
```

#### Nerd Font (icon-rich)
```rust
 ~/projects/sol  main  2.3s
```
Uses Nerd Font icons; informative and visually appealing.

```toml
[prompt]
preset = "nerd-font"
```

#### Developer
```rust
λ ~/projects/sol (main* ↑1) [node 20.10.0] 2.3s
```
Shows more context: Git status, environment versions, execution time, and more.

```toml
[prompt]
preset = "developer"
```

### Advanced Customization

Fully customize the prompt using template syntax:

```toml
[prompt]
format = """
$symbol $directory $git_branch $git_status
$duration $status $character
"""

# Or use a single line
format = "$symbol $directory $git_branch $duration"
```

**Available variables:**
- `$symbol` - Prompt symbol
- `$user` - Current username
- `$host` - Hostname
- `$directory` - Current directory
- `$git_branch` - Git branch name
- `$git_status` - Git status (clean/dirty/conflict)
- `$git_ahead_behind` - Commits ahead/behind upstream
- `$duration` - Execution time of the previous command
- `$status` - Exit code of the previous command
- `$jobs` - Number of background jobs
- `$env` - Environment info (virtual environment, container, etc.)
- `$time` - Current time
- `$character` - A character that changes based on state

### Conditional Display

Some elements are shown only under certain conditions:

```toml
[prompt.git_branch]
# Show only inside a Git repository
show_always = false

[prompt.duration]
# Show only when a command takes longer than 1 second
threshold = 1000  # milliseconds

[prompt.status]
# Show the exit code only when a command fails
show_on_success = false
```

## Multi-Line Prompts

For complex commands, Lyra supports multi-line input:

```rust
λ curl https://api.example.com/data \
│   | jq '.items[]' \
│   | where .status == "active"
```

- First line: the normal prompt `λ`
- Subsequent lines: the continuation symbol `│` (customizable)

## Dynamic Prompts

### Git Integration

Inside a Git repository, the prompt automatically shows the branch and status:

```rust
λ ~/projects/sol (main)           # Clean main branch
λ ~/projects/sol (feature/lyra*)  # Uncommitted changes present
λ ~/projects/sol (main↑2)         # 2 commits ahead of upstream
λ ~/projects/sol (main↓1)         # 1 commit behind upstream
λ ~/projects/sol (main⚡)         # Conflicts present
```

### Environment Detection

Lyra automatically detects and displays special environments:

```rust
λ (venv) ~/python-project              # Python virtual environment
λ  ~/rust-project                     # Rust project (Cargo.toml)
λ  ~/node-project                     # Node.js project
λ 🐳 /app                              # Inside a Docker container
λ ☁️  ~/aws-project                    # AWS credentials loaded
```

## Performance Optimization

Prompt rendering should be fast (< 50 ms). Lyra uses the following strategies:

1. **Lazy evaluation** - Git status and similar info are computed only when needed
2. **Caching** - Directory and Git information is cached briefly
3. **Async rendering** - Slow operations (network checks) update asynchronously
4. **Configurable timeouts** - Slow operations are abandoned after a timeout

```toml
[prompt.performance]
# Git status computation timeout (milliseconds)
git_timeout = 100

# Directory info cache duration (seconds)
cache_duration = 5

# Disable specific slow features
disable_git_upstream_check = false
```

## Integration with the SOL Design System

Prompt colors use SOL design tokens to ensure consistency with the system theme:

```toml
[prompt.colors]
# Reference sol-design tokens directly
symbol = "accent-primary"
directory = "accent-success"
error = "accent-error"
```

When the system theme switches (light/dark), prompt colors adapt automatically.

## Accessibility

### Color Blindness Friendly

Beyond color, Lyra uses symbols to distinguish states:
- ✓ Success - `λ` + green
- ✗ Failure - `λ!` + red
- ⚠ Warning - `λ*` + yellow

### Screen Readers

The prompt is semantic and screen-reader friendly:
```
"Lambda prompt, directory home projects sol, git branch main with uncommitted changes"
```

### High Contrast Mode

```toml
[prompt]
high_contrast = true  # Use stronger color contrast
```

## Quick Configuration from the Command Line

Don't want to edit a config file? Adjust settings directly from the command line:

```rust
# Change the prompt symbol
λ lyra config set prompt.symbol "❯"

# Switch themes
λ lyra config set prompt.preset nerd-font

# Disable Git info
λ lyra config set prompt.git_branch.show_always false

# View the current configuration
λ lyra config get prompt

# Reset to defaults
λ lyra config reset prompt
```

## Example Configurations

### My Configuration (author's recommendation)

```toml
[prompt]
format = "$symbol $directory $git_branch $duration"
symbol = "λ"
symbol_error = "λ!"

[prompt.colors]
symbol = "#38BDF8"
symbol_error = "#E9A568"
directory = "#6EE7B7"
git_branch = "#A78BFA"

[prompt.duration]
threshold = 1000
show_always = false
```

### Minimalist

```toml
[prompt]
preset = "minimalist"
```

### Information Lover

```toml
[prompt]
preset = "developer"
format = """
┌─ $user@$host $time
├─ $directory
├─ $git_branch $git_status $git_ahead_behind
└─ $symbol
"""
```

## FAQ

### Q: How do I type λ?

A: You don't need to! The prompt is displayed by Lyra; you only type commands.

### Q: My font doesn't support λ. What should I do?

A: Install a Unicode-capable monospace font (JetBrains Mono, Fira Code, and Cascadia Code are recommended), or switch to a different symbol:

```bash
lyra config set prompt.symbol ">"
```

### Q: Can the prompt be displayed on the right side?

A: A right-side prompt (rprompt) is not supported yet, but it is planned for Phase 2.

### Q: Prompt rendering is slow. What should I do?

A: Check whether Git status detection is the bottleneck:

```bash
# Temporarily disable Git info
lyra config set prompt.git_branch.show_always false

# Or increase the timeout
lyra config set prompt.performance.git_timeout 50
```

### Q: Can I run custom commands in the prompt?

A: Phase 3 will support custom prompt modules:

```rust
# Future feature
[prompt.modules.custom]
command = "my-prompt-info"
timeout = 100
```

## Summary

Lyra's prompt design embodies the project's core philosophy:
- **Elegant** - The λ symbol is clean and meaningful
- **Intelligent** - Automatically detects context (Git, environment, errors)
- **Consistent** - Integrated with the SOL design system
- **Flexible** - From minimalist to information-rich, fully customizable

The default configuration works well for most users while giving advanced users powerful customization capabilities.

The prompt is not mere decoration - it is the first interface between the shell and the user.
