# Lyra Intelligence Features

Lyra is not just a shell with nicer syntax; it is an intelligent assistant. Through context awareness, machine learning, and deep system integration, Lyra makes command-line interaction smoother, safer, and more efficient.

## Smart Completion

### Context-Aware Completion

Lyra understands the current context and provides precise completion suggestions:

#### File Path Completion

```rust
λ cat ~/pro[Tab]
# Expands to
λ cat ~/projects/

λ cat ~/projects/SOL/comp[Tab]
# Shows the completion menu
compositor/   compat/   

λ ls *.rs[Tab]
# Expands to all .rs files
λ ls main.rs parser.rs runtime.rs
```

**Features:**
- Fuzzy matching: `~/pr/so` → `~/projects/sol`
- Case-insensitive (configurable)
- Wildcard expansion support
- File type icons (directories, files, links)

#### Git Integration Completion

```rust
λ git checkout [Tab]
# Shows the branch list with status indicators
main                 # Current branch
feature/lyra         # Local branch
fix/parser-bug       # Local branch
origin/dev           # Remote branch

λ git merge feat[Tab]
# Completes to
λ git merge feature/lyra

λ git log origin/[Tab]
# Completes remote branches
origin/main
origin/dev
origin/staging
```

#### Docker/Container Completion

```rust
λ docker exec [Tab]
# Shows running containers
elegant_tesla    (nginx:latest)
brave_newton     (postgres:14)
zen_turing       (redis:alpine)

λ docker logs zen[Tab]
# Completes to
λ docker logs zen_turing
```

#### System Service Completion

```rust
λ systemctl status [Tab]
# Shows the service list with status
● networkd.service      (active)
● sshd.service          (active)
○ bluetooth.service     (inactive)

λ systemctl restart net[Tab]
# Completes to
λ systemctl restart networkd.service
```

#### Process Completion

```rust
λ kill [Tab]
# Shows the process list
1234  firefox         (12% CPU, 1.2GB)
5678  code            (8% CPU, 800MB)
9012  sol-compositor  (2% CPU, 150MB)

λ kill 90[Tab]
# Completes to
λ kill 9012
```

### Command Argument Completion

Lyra knows the argument structure of every command:

```rust
λ cargo [Tab]
build    check    run      test     doc      clean
new      init     add      update   publish

λ cargo run -p [Tab]
# Shows the packages in the workspace
sol-compositor
sol-shell
sol-ui
sol-design

λ systemctl [Tab]
start    stop     restart  status   enable   disable
list-units   list-unit-files   show

λ git [Tab]
add      commit   push     pull     status   log
branch   checkout merge    rebase   stash    diff
```

### Inline Preview

Shows a live preview while completing:

```rust
λ cat README[Tab]
# Shows a file content preview
README.md
┌─────────────────────────────────┐
│ # SOL Operating System          │
│                                  │
│ SOL is a Linux Family OS...     │
│                                  │
│ ## Quick Start                   │
│ ...                              │
└─────────────────────────────────┘

λ git log [Tab]
# Shows a preview of recent commits
--oneline
┌─────────────────────────────────┐
│ a3f8b92 Add lyra documentation  │
│ 7c2d1e5 Implement prompt system │
│ 9f4a8b3 Add structured data     │
└─────────────────────────────────┘

λ systemctl status network[Tab]
networkd.service
┌─────────────────────────────────┐
│ ● networkd.service - Network    │
│   Loaded: loaded                │
│   Active: active (running)      │
│   Memory: 12.3M                 │
└─────────────────────────────────┘
```

### Smart Suggestions

Proactive suggestions based on history and context:

```rust
λ cd ~/projects/sol
# Lyra remembers what you usually do next

λ [empty input, suggestions shown]
Suggested commands based on history:
  cargo build --workspace
  cargo run -p sol-compositor
  git status

λ git commit
# Lyra detects unstaged files
Warning: You have unstaged changes
Suggestion: git add -A && git commit
[Enter] to accept  [Tab] to edit  [Esc] to cancel
```

### Fuzzy Search

Quickly find commands using fuzzy matching:

```rust
λ [Ctrl+R]  # Enter history search mode
> crg bld ws_____
# Matches
cargo build --workspace

λ [Ctrl+T]  # Fuzzy file search
> comp/sr/ma____
# Matches
compositor/src/main.rs
```

### Completion Configuration

```toml
# ~/.config/lyra/config.toml
[completion]
# Enable fuzzy matching
fuzzy = true

# Case sensitivity
case_sensitive = false

# Inline preview
inline_preview = true

# Maximum number of completion candidates
max_candidates = 50

# Preview window height
preview_height = 10

# Auto-completion delay (milliseconds)
delay = 100
```

## Smart Error Correction

### Spell Correction

Lyra automatically detects common spelling mistakes:

```rust
λ car build
Did you mean: cargo build
[Enter] to run  [Esc] to edit  [Tab] for alternatives

Alternatives:
  1. cargo build
  2. car (not installed)
  3. cd build/

λ gti status
Did you mean: git status
Auto-correcting in 3s... [Esc] to cancel
```

### Argument Correction

```rust
λ git comit
Error: Unknown command 'comit'
Did you mean: git commit
Suggestion: git commit

λ cargo biuld
Error: Unknown command 'biuld'
Did you mean: cargo build

λ systemctl statsu
Error: Unknown command 'statsu'
Did you mean: systemctl status
```

### Path Correction

```rust
λ cd ~/projets
Error: Directory not found: ~/projets
Did you mean: ~/projects

λ cat ~/projects/SOL/READM.md
Error: File not found: READM.md
Did you mean: README.md
```

### Permission Hints

```rust
λ systemctl restart networkd
Error: Permission denied
Suggestion: Try with sudo?
  sudo systemctl restart networkd
[Enter] to retry with sudo  [Esc] to cancel

λ rm /etc/hosts
Error: Permission denied
Warning: This is a system file!
Are you sure? [y/N]
```

### Dangerous Command Warnings

```rust
λ rm -rf /
ERROR: Destructive command on root directory!
This command is blocked for safety.

λ dd if=/dev/zero of=/dev/sda
WARNING: This will DESTROY all data on /dev/sda!
Type 'yes I understand' to continue: _

λ chmod 777 ~/.ssh
WARNING: Setting 777 on ~/.ssh is insecure!
Suggestion: chmod 700 ~/.ssh
Continue anyway? [y/N]
```

## Smart History

### Context-Aware History

History entries are associated with the context in which they ran:

```rust
λ [Up]  # Show history for the current directory
# In ~/projects/sol, shows:
cargo build --workspace
cargo test -p sol-compositor
git status
cargo run -p sol-compositor

# Switch to another directory and the history switches too
λ cd ~/documents
λ [Up]  # Shows history for that directory
cat report.txt
grep "TODO" *.md
```

### Time-Aware History

```rust
λ [Ctrl+R]
> [time filter]

Show history from:
  1. Last hour
  2. Today
  3. This week
  4. This month
  5. All time

λ history --today
# Shows today's commands
14:30  git commit -m "Add feature"
15:45  cargo test
16:20  git push
```

### Smart Deduplication

```rust
# Traditional shells: duplicate commands pile up
λ history
1  ls
2  ls
3  ls -l
4  ls
5  ls -l
6  cd ..
7  ls

# Lyra: smart deduplication, keeping the most recent
λ history
1  ls -l          (used 2 times)
2  ls             (used 3 times)
3  cd ..
```

### History Statistics

```rust
λ history stats
Top commands:
  1. git status        (234 times)
  2. cargo build       (189 times)
  3. ls                (167 times)
  4. cd                (143 times)
  5. cargo test        (98 times)

Most used in ~/projects/sol:
  1. cargo build --workspace
  2. git status
  3. cargo test

Failed commands (last week):
  1. cargo build        (3 times)
  2. systemctl restart  (2 times)
```

### History Sync

```rust
# Cross-session history sync
[Terminal 1]
λ git pull

[Terminal 2]
λ [Up]  # Immediately see Terminal 1's command
git pull
```

## Command Preview

Preview a command's effect before running it:

### File Operation Preview

```rust
λ rm *.log
Preview: Will delete 12 files (2.3 MB)
  error.log      (512 KB)
  debug.log      (1.2 MB)
  access.log     (600 KB)
  ...
[Enter] to confirm  [Esc] to cancel

λ mv *.rs src/
Preview: Will move 8 files to src/
  main.rs       → src/main.rs
  parser.rs     → src/parser.rs
  runtime.rs    → src/runtime.rs
  ...
[Enter] to confirm
```

### Network Operation Preview

```rust
λ curl https://api.example.com/large-file.zip
Preview:
  URL: https://api.example.com/large-file.zip
  Size: 1.2 GB
  Content-Type: application/zip
  Estimated time: 5m 30s
Download? [Y/n]
```

### System Operation Preview

```rust
λ systemctl stop networkd
Preview:
  Service: networkd.service
  Impact: Will disconnect network
  Dependent services: 3
    - sshd.service (will remain active)
    - docker.service (may lose connectivity)
Continue? [y/N]
```

## Environment Detection

Lyra automatically detects and adapts to the current environment:

### Git Repository Detection

```rust
λ cd ~/projects/sol
# Automatically detects the Git repository; the prompt shows the branch
λ (main)

# Provides Git-related completions
λ [Tab]
# Automatically includes Git command suggestions
git status
git commit
git push
```

### Project Type Detection

```rust
λ cd ~/rust-project
# Detects Cargo.toml
Environment: Rust project
Available: cargo build, cargo test, cargo run

λ [Tab]
# Automatically includes Cargo commands
cargo build
cargo test
cargo clippy

λ cd ~/node-project
# Detects package.json
Environment: Node.js project
Available: npm install, npm run, npm test

λ [Tab]
# Automatically includes npm/yarn commands
npm install
npm run dev
yarn build
```

### Container Environment Detection

```rust
λ cd ~/docker-project
# Detects Dockerfile and docker-compose.yml
Environment: Docker project
Available: docker build, docker-compose up

λ [Inside container]
λ (🐳 web-container)
# The prompt shows the container name
# Commands adapt to the container environment
```

### Python Virtual Environments

```rust
λ cd ~/python-project
λ source venv/bin/activate
# Lyra automatically detects the virtual environment
λ (venv)

# Provides virtual-environment-related commands
λ pip[Tab]
pip install
pip list
pip freeze
```

## Syntax Highlighting

Real-time syntax highlighting for better readability:

```rust
# Commands: yellow
λ echo "hello"

# Strings: green
λ echo "hello world"

# Variables: cyan
λ echo $name

# Errors: red
λ unknowncommand
  ^^^^^^^^^^^^^^
  Error: Command not found

# Existing paths: green; nonexistent: red
λ cat /existing/path    # Green
λ cat /invalid/path     # Red
```

## Auto-Suggestions (Fish-style)

History-based auto-suggestions, shown in gray:

```rust
λ git st
       atus  # Gray suggestion, press → to accept

λ cargo b
         uild --workspace  # Full command suggestion based on history

λ cd ~/pro
          jects/sol  # Path auto-suggestion
```

**Key bindings:**
- `→` (right arrow) - Accept the entire suggestion
- `Ctrl+→` - Accept one word
- `Esc` - Dismiss the suggestion

## Smart Alias Learning

Lyra learns your usage patterns and proactively suggests aliases:

```rust
λ git status
# After 50 uses
Lyra noticed: You use 'git status' frequently
Suggestion: Create alias 'gst' for 'git status'?
[Y/n]

λ y
Alias created: gst → git status

# Afterwards
λ gst
# Equivalent to git status
```

## Error Explanation

Lyra doesn't just tell you an error occurred; it explains why and how to fix it:

```rust
λ cargo build
Error: could not compile `sol-compositor`

Explanation:
  Missing semicolon at line 145 in src/main.rs
  
Suggestion:
  Add ';' after the expression
  
Location:
  src/main.rs:145:30
  
Quick fix:
  [Enter] to open in editor  [Esc] to cancel

λ systemctl status networkd
Error: Failed to connect to bus

Explanation:
  D-Bus daemon is not running or not accessible
  
Possible causes:
  1. Not enough permissions (try sudo)
  2. D-Bus service not started
  3. DBUS_SESSION_BUS_ADDRESS not set
  
Suggestions:
  • Check: systemctl status dbus
  • Try: sudo systemctl status networkd
```

## Performance Monitoring

Shows command execution information in real time:

```rust
λ cargo build --workspace
Building... [████████░░] 80% (12/15 crates)
Time: 45s  CPU: 340%  Memory: 1.2GB

λ git clone https://github.com/large/repo
Cloning... [████░░░░░░] 40% (245 MB / 610 MB)
Speed: 8.2 MB/s  ETA: 45s
```

## AI Assistance (Future Feature)

### Natural Language Commands

```rust
λ lyra help "find large files modified today"
Suggested command:
  find . -type f -mtime 0 -size +10M

λ lyra suggest "compress all logs older than 7 days"
Suggested command:
  find . -name "*.log" -mtime +7 -exec gzip {} \;

Explanation:
  • find . -name "*.log": Find all .log files
  • -mtime +7: Modified more than 7 days ago
  • -exec gzip {}: Compress each file

Run this command? [Y/n/edit]
```

### Error Diagnosis

```rust
λ cargo build
Error: linker error...

λ lyra diagnose
Analyzing error...

Diagnosis:
  Missing system library: libssl-dev
  
Solution:
  Install the library:
    Ubuntu/Debian: sudo apt install libssl-dev
    Arch: sudo pacman -S openssl
    macOS: brew install openssl

Would you like me to:
  1. Install the library (requires sudo)
  2. Show manual installation steps
  3. Search for more solutions
```

## Configuration

All intelligence features are configurable:

```toml
# ~/.config/lyra/config.toml
[intelligence]
# Enable smart completion
completion = true

# Enable spell correction
spell_check = true

# Enable inline suggestions
auto_suggest = true

# Enable syntax highlighting
syntax_highlight = true

# Enable command preview
command_preview = true

# Dangerous command confirmation
dangerous_command_check = true

# Alias learning threshold (number of command uses)
alias_suggestion_threshold = 50

# AI assistance (requires an API key)
ai_assist = false
ai_provider = "openai"  # openai, anthropic, local
ai_api_key = "sk-..."
```

## Performance Considerations

The implementation of intelligence features is designed to avoid impacting performance:

- **Async computation** - Completions and suggestions are computed on background threads
- **Caching** - Git status, file listings, and similar information are cached briefly
- **Progressive rendering** - Large candidate sets are rendered in batches
- **Timeouts** - Slow operations time out automatically
- **Configurable** - Individual features can be disabled to improve performance

```toml
[intelligence.performance]
# Completion timeout (milliseconds)
completion_timeout = 200

# Git status cache duration (seconds)
git_cache_duration = 5

# Maximum number of completion candidates
max_completion_items = 100

# Disable specific features
disable_inline_preview = false
disable_auto_suggest = false
```

## Summary

Lyra's intelligence features shift the command line from "memory-driven" to "assistance-driven":

- **Completion** - No need to memorize every command and argument
- **Error correction** - Automatically fixes common mistakes
- **Preview** - See the effect before executing
- **Suggestions** - Proactive recommendations based on context
- **Learning** - Adapts to your usage habits

These features work together to make Lyra a true intelligent assistant, not just a command interpreter.
