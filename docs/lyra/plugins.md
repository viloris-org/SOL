# Lyra Plugin System

The plugin system lets users and third-party developers extend Lyra with custom commands, completers, and other features while maintaining security and isolation.

## Design Philosophy

1. **Easy to write** - Plugins are written in Lyra syntax; no compilation needed
2. **Secure isolation** - Plugins run in a restricted environment and cannot break the system
3. **Automatic discovery** - Plugins placed in standard directories are loaded automatically
4. **Namespacing** - Avoids command name conflicts
5. **Composable** - Plugins can use functionality from other plugins

## Plugin Types

### 1. Command Plugins

Add new commands:

```rust
# plugins/git-utils.ly

## Git utilities plugin
##
## Provides convenient Git operation commands

export def git-recent [--count: int = 10] {
    ## Show recent commits
    ##
    ## Parameters:
    ##   --count: number of commits to show (default 10)
    
    git log --oneline -n $count | parse "{hash} {message}"
}

export def git-branches [] {
    ## List all branches with their last commit info
    
    git branch -v | parse "{current} {name} {hash} {message}"
}

export def git-clean-merged [] {
    ## Delete merged local branches
    
    let branches = (git branch --merged | where name != "main" and name != "master")
    
    if ($branches | count) == 0 {
        echo "No merged branches to clean"
        return
    }
    
    echo "Will delete:"
    $branches | each { echo "  - ${it.name}" }
    
    let confirm = (input "Continue? [y/N]: ")
    if $confirm == "y" {
        $branches | each { git branch -d ${it.name} }
        echo "Done!"
    }
}

export def git-uncommitted [] {
    ## Find all repositories with uncommitted changes
    
    ls --recursive \
        | where name == ".git" \
        | each { 
            let repo = (${it.path} | path-dirname)
            cd $repo
            let status = (git status --short)
            if ($status | length) > 0 {
                {repo: $repo, changes: $status}
            }
        }
}
```

### 2. Completion Plugins

Add completion support for specific commands:

```rust
# plugins/docker-complete.ly

export def complete-docker [position: int, line: string] {
    ## Docker command completion
    
    let words = ($line | split " ")
    let cmd = ($words | get 1)
    
    match $position {
        1 => {
            # Subcommand completion
            ["run", "exec", "ps", "images", "build", "pull", "push", "logs", "stop", "rm"]
        },
        2 => {
            # Argument completion based on the subcommand
            match $cmd {
                "exec" | "logs" | "stop" | "rm" => {
                    # Container name completion
                    docker ps --format "{{.Names}}"
                },
                "rmi" => {
                    # Image completion
                    docker images --format "{{.Repository}}:{{.Tag}}"
                },
                _ => []
            }
        },
        _ => []
    }
}
```

### 3. Theme Plugins

Customize the prompt and colors:

```rust
# plugins/themes/neon.ly

export def theme-neon [] {
    ## Neon theme - vivid cyberpunk style
    
    config set prompt.colors.symbol "#FF00FF"        # Magenta
    config set prompt.colors.directory "#00FFFF"     # Cyan
    config set prompt.colors.git_branch "#FFFF00"    # Yellow
    config set prompt.colors.duration "#00FF00"      # Green
    config set prompt.symbol "⚡"
    
    echo "Neon theme activated!"
}

export def theme-minimal [] {
    ## Minimal theme - black and white
    
    config set prompt.colors.symbol "#FFFFFF"
    config set prompt.colors.directory "#CCCCCC"
    config set prompt.colors.git_branch "#999999"
    config set prompt.symbol ">"
    config set prompt.git_branch.show_always false
    
    echo "Minimal theme activated!"
}
```

## Plugin Structure

### Basic Structure

```rust
# plugins/my-plugin.ly

## Plugin metadata (optional but recommended)
##
## name: my-plugin
## version: 1.0.0
## author: Your Name
## description: A useful plugin for Lyra

# Private function (not exported)
def helper-function [] {
    # Used only inside the plugin
}

# Exported command (callable by users)
export def public-command [arg: string] {
    ## Documentation comment for the public command
    ##
    ## Parameters:
    ##   arg: parameter description
    
    # Implementation
    helper-function
    echo "Processing: $arg"
}

# Plugin initialization (optional)
def --init [] {
    ## Runs automatically when the plugin is loaded
    
    # Set environment variables
    env MY_PLUGIN_CONFIG "~/.config/my-plugin"
    
    # Create required directories
    mkdir -p $MY_PLUGIN_CONFIG
    
    echo "my-plugin loaded"
}

# Plugin cleanup (optional)
def --cleanup [] {
    ## Runs when the plugin is unloaded
    
    echo "my-plugin unloaded"
}
```

### Namespacing

Plugin commands are automatically placed in a namespace to avoid conflicts:

```rust
# plugins/git-utils.ly
export def recent [] { ... }

# Usage
λ git-utils.recent      # Full namespace
λ use git-utils         # After importing, use directly
λ recent
```

## Plugin Management

### Installation Locations

Plugins can be placed in the following directories:

```
~/.config/lyra/plugins/          # User plugins
/usr/share/lyra/plugins/         # System plugins (requires sudo)
./lyra_modules/                  # Project-local plugins
```

### Plugin Commands

```rust
# List all plugins
λ plugin list
╭───┬─────────────┬─────────┬────────────────────╮
│ # │    name     │ version │    description     │
├───┼─────────────┼─────────┼────────────────────┤
│ 0 │ git-utils   │ 1.0.0   │ Git utilities      │
│ 1 │ docker-comp │ 0.5.0   │ Docker completions │
╰───┴─────────────┴─────────┴────────────────────╯

# Install a plugin (from a URL)
λ plugin install https://raw.githubusercontent.com/user/lyra-plugin/main/plugin.ly

# Install a plugin (from a file)
λ plugin install ~/downloads/my-plugin.ly

# Uninstall a plugin
λ plugin remove git-utils

# Update all plugins
λ plugin update

# Search for plugins
λ plugin search git
╭───┬──────────────┬────────────────────────────╮
│ # │     name     │        description         │
├───┼──────────────┼────────────────────────────┤
│ 0 │ git-utils    │ Enhanced Git utilities     │
│ 1 │ git-flow     │ Git-flow workflow helpers  │
╰───┴──────────────┴────────────────────────────╯

# Show plugin info
λ plugin info git-utils
Name: git-utils
Version: 1.0.0
Author: Lyra Community
Description: Enhanced Git utilities
Commands:
  - recent: Show recent commits
  - branches: List branches with info
  - clean-merged: Delete merged branches
```

### Using Plugins

```rust
# Method 1: full namespace
λ git-utils.recent

# Method 2: import the namespace
λ use git-utils
λ recent
λ branches

# Method 3: import specific commands
λ use git-utils {recent, branches}
λ recent

# Method 4: use an alias
λ use git-utils as gu
λ gu.recent
```

## Practical Plugin Examples

### Developer Tools Plugin

```rust
# plugins/dev-tools.ly

export def project-info [] {
    ## Show information about the current project
    
    let info = {
        path: (pwd),
        git: if (ls .git | count) > 0 {
            {
                branch: (git branch --show-current),
                status: (git status --short),
                remote: (git remote get-url origin)
            }
        } else { null },
        language: if (ls Cargo.toml | count) > 0 {
            "Rust"
        } else if (ls package.json | count) > 0 {
            "Node.js"
        } else if (ls go.mod | count) > 0 {
            "Go"
        } else {
            "Unknown"
        }
    }
    
    $info
}

export def watch-build [--command: string = "cargo build"] {
    ## Watch for file changes and rebuild automatically
    ##
    ## Parameters:
    ##   --command: the build command to run
    
    echo "Watching for changes..."
    
    while true {
        # Wait for file changes (simplified; real implementation needs inotify support)
        sleep 1s
        
        # Check whether any .rs files changed
        let changed = (ls src/*.rs | where modified > (date now - 2s))
        
        if ($changed | count) > 0 {
            echo "Changes detected, rebuilding..."
            $command
        }
    }
}

export def clean-artifacts [] {
    ## Clean up various build artifacts
    
    let patterns = [
        "target/",
        "node_modules/",
        "*.pyc",
        "__pycache__/",
        "*.o",
        "*.so"
    ]
    
    $patterns | each { |pattern|
        let files = (ls --recursive $pattern)
        if ($files | count) > 0 {
            echo "Removing: $pattern"
            rm -r $pattern
        }
    }
}
```

### System Administration Plugin

```rust
# plugins/sys-admin.ly

export def port-check [port: int] {
    ## Check whether a port is in use
    ##
    ## Parameters:
    ##   port: port number
    
    let result = (netstat -tuln | where local_port == $port)
    
    if ($result | count) > 0 {
        echo "Port $port is in use:"
        $result
    } else {
        echo "Port $port is available"
    }
}

export def service-status [] {
    ## Show the status of key services
    
    let services = [
        "sshd",
        "networkd",
        "systemd-resolved",
        "docker"
    ]
    
    $services | each { |svc|
        let status = (systemctl status $svc)
        {
            service: $svc,
            status: $status.active_state,
            memory: $status.memory_current
        }
    }
}

export def disk-usage [--threshold: int = 80] {
    ## Show disk usage, highlighting entries above the threshold
    ##
    ## Parameters:
    ##   --threshold: usage threshold (percentage)
    
    df | where use_percent > $threshold | sort-by use_percent -r
}

export def backup [source: path, dest: path] {
    ## Smart directory backup
    ##
    ## Parameters:
    ##   source: source directory
    ##   dest: destination directory
    
    let timestamp = (date now | date-format "%Y%m%d_%H%M%S")
    let backup_name = "${dest}/backup_${timestamp}"
    
    echo "Backing up ${source} to ${backup_name}..."
    
    cp -r $source $backup_name
    
    # Compress the backup
    tar czf "${backup_name}.tar.gz" $backup_name
    rm -r $backup_name
    
    echo "Backup completed: ${backup_name}.tar.gz"
}
```

### Data Processing Plugin

```rust
# plugins/data-tools.ly

export def csv-merge [files: list] {
    ## Merge multiple CSV files
    ##
    ## Parameters:
    ##   files: list of CSV file paths
    
    let all_data = []
    
    $files | each { |file|
        let data = (cat $file | from-csv)
        let all_data = ($all_data | append $data)
    }
    
    $all_data
}

export def json-flatten [--separator: string = "."] {
    ## Flatten nested JSON objects
    ##
    ## Parameters:
    ##   --separator: key name separator
    
    # Recursive flatten function
    def flatten-object [obj: record, prefix: string = ""] {
        let result = {}
        
        $obj | each { |key, value|
            let new_key = if $prefix == "" { $key } else { "${prefix}${separator}${key}" }
            
            if ($value | type) == "record" {
                let result = ($result | merge (flatten-object $value $new_key))
            } else {
                let result = ($result | merge {$new_key: $value})
            }
        }
        
        $result
    }
    
    $in | flatten-object
}

export def table-pivot [row_col: string, col_col: string, val_col: string] {
    ## Pivot a table
    ##
    ## Parameters:
    ##   row_col: row dimension column name
    ##   col_col: column dimension column name  
    ##   val_col: value column name
    
    let data = $in
    let row_values = ($data | select $row_col | unique)
    let col_values = ($data | select $col_col | unique)
    
    # Build the pivot table
    # Implementation omitted (requires more complex logic)
}
```

## Security Sandbox

Plugins run in a restricted environment with the following limitations:

### Filesystem Access

```rust
# Allowed: user directories and the current directory
λ ls ~/documents        # ✓
λ ls .                  # ✓

# Requires confirmation: system directories
λ ls /etc              # Asks for confirmation

# Forbidden: sensitive directories
λ cat /etc/shadow      # ✗ Permission denied
```

### Network Access

```rust
# Requires confirmation: HTTP requests
λ curl https://api.example.com    # Asks for confirmation

# Plugins can declare the permissions they need
## permissions: network, filesystem:/home/user/data
```

### Process Execution

```rust
# Allowed: built-in commands and commonly installed tools
λ git status           # ✓
λ cargo build          # ✓

# Requires confirmation: uncommon commands
λ /tmp/suspicious      # Asks for confirmation

# Forbidden: clearly dangerous operations
λ rm -rf /             # ✗ Blocked
```

## Plugin Configuration

Plugins can have their own configuration files:

```toml
# ~/.config/lyra/plugins/git-utils.toml
[git-utils]
default_branch = "main"
auto_fetch = true
show_status_in_prompt = true
```

Reading configuration inside a plugin:

```rust
# plugins/git-utils.ly

export def recent [] {
    let config = (config get plugins.git-utils)
    let count = $config.recent_count or 10
    
    git log --oneline -n $count | parse "{hash} {message}"
}
```

## Publishing Plugins

### Creating a Distributable Plugin

```bash
# Plugin directory structure
my-plugin/
├── plugin.ly          # Main file
├── README.md          # Documentation
├── LICENSE            # License
├── config.toml        # Default configuration
└── tests/             # Tests
    └── test.ly
```

### Publishing to a Plugin Registry

```rust
# Package a plugin
λ plugin package my-plugin/

# Publish to the official registry (requires authentication)
λ plugin publish my-plugin-1.0.0.lyra-plugin

# Or publish to GitHub
# Users can install via URL
λ plugin install https://raw.githubusercontent.com/user/repo/main/plugin.ly
```

## Plugin Development Best Practices

### 1. Thorough Documentation

```rust
export def my-command [arg: string, --flag: bool = false] {
    ## Short description of the command (one line)
    ##
    ## More detailed explanation, possibly multi-line.
    ## Explain what the command does and how it behaves.
    ##
    ## Parameters:
    ##   arg: parameter description
    ##   --flag: flag description
    ##
    ## Examples:
    ##   my-command "example"
    ##   my-command "test" --flag
    ##
    ## Returns:
    ##   Description of the return value
    
    # Implementation
}
```

### 2. Error Handling

```rust
export def safe-command [path: path] {
    try {
        if not (path-exists $path) {
            error "Path not found: $path"
        }
        
        # Operation
    } catch {
        echo "Error: ${error.message}"
        return null
    }
}
```

### 3. Testing

```rust
# tests/test.ly

def test-my-command [] {
    let result = (my-command "test")
    assert ($result != null)
    assert ($result.status == "ok")
}

# Run tests
λ plugin test my-plugin
```

### 4. Version Compatibility

```rust
## requires: lyra >= 0.2.0
##
## Declares the minimum Lyra version required by the plugin
```

### 5. Performance Optimization

```rust
# Cache expensive computations
export def expensive-operation [] {
    let cache_file = "~/.cache/lyra/my-plugin/result.json"
    
    if (path-exists $cache_file) {
        let age = ((path-mtime $cache_file) - (date now))
        if $age < 1h {
            return (cat $cache_file | from-json)
        }
    }
    
    # Perform the expensive operation
    let result = ...
    
    # Save the cache
    $result | to-json | save $cache_file
    
    $result
}
```

## Official Plugin Collection

Lyra plans to offer a set of officially maintained plugins:

- **lyra-git** - Enhanced Git tooling
- **lyra-docker** - Docker management and completion
- **lyra-k8s** - Kubernetes integration
- **lyra-aws** - AWS CLI enhancements
- **lyra-dev** - Developer tool collection
- **lyra-data** - Data analysis tooling
- **lyra-themes** - Theme collection

## Summary

Lyra's plugin system provides powerful and secure extensibility:

- **Easy to develop** - Written in Lyra syntax, no compilation needed
- **Automatically managed** - Installing, updating, and uninstalling are all simple
- **Secure isolation** - The sandbox mechanism protects the system
- **Community-driven** - Users are encouraged to share and contribute plugins

This lets Lyra adapt to a wide range of scenarios, from everyday tasks to professional development, with the right tool always available.
