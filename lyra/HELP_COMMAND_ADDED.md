# Help Command Implementation

## Summary
Added a comprehensive `help` command to Lyra shell to match Fish shell's built-in help functionality.

## Changes Made

### 1. Extended Builtin Trait (`src/builtins/registry.rs`)
- Added `description()` method to the `Builtin` trait with a default implementation
- Added helper methods to `BuiltinRegistry`:
  - `get_command(name)` - Get a specific command by name
  - `all_commands()` - Get all commands with their descriptions

### 2. Created Help Command (`src/builtins/basic.rs`)
- New `Help` builtin that displays:
  - **Without arguments**: Lists all available commands organized by category:
    - Basic Commands (echo, pwd, cd, exit, ls, which, clear, reset, help)
    - File Operations (cat, cp, mv, rm, mkdir, touch)
    - Text Utilities (grep, head, tail, wc, sort, uniq)
    - System Utilities (env, basename, dirname, sleep, date, true, false, whoami, uname)
  - **With argument**: Shows help for a specific command

### 3. Added Descriptions to All Commands
Added descriptive text to every builtin command:

**Basic Commands:**
- echo - Print arguments to stdout
- pwd - Print current working directory
- cd - Change current directory
- exit - Exit the shell
- ls - List directory contents
- which - Locate a command
- clear - Clear the terminal screen
- reset - Reset the terminal to initial state
- help - Display help information about builtin commands

**File Operations:**
- cat - Concatenate and display file contents
- cp - Copy files and directories
- mv - Move or rename files and directories
- rm - Remove files or directories
- mkdir - Create directories
- touch - Create empty files or update timestamps

**Text Utilities:**
- head - Output the first part of files
- tail - Output the last part of files
- wc - Count lines, words, and characters
- grep - Search for patterns in files
- sort - Sort lines of text files
- uniq - Report or filter out repeated lines

**System Utilities:**
- env - Display environment variables
- basename - Strip directory from filename
- dirname - Strip last component from filename
- sleep - Delay for a specified amount of time
- date - Display the system date and time
- true - Return success (exit code 0)
- false - Return failure (exit code 1)
- whoami - Print current username
- uname - Print system information

### 4. Updated Exports
- Added `Help` to the exports in `src/builtins/mod.rs`
- Registered `Help` command in the builtin registry

## Usage

```bash
# Show all available commands organized by category
help

# Show help for a specific command
help ls
help cd
help grep
```

## Features
- Color-coded output with ANSI escape sequences (cyan for command names)
- Commands aligned in columns for easy reading
- Organized by logical categories
- Helpful message at the end: "Type 'help <command>' for more information on a specific command."

## Testing
To test the help command, run Lyra shell:

```bash
cargo run -p lyra
```

Then type:
```
help
help ls
help grep
```

The help command now provides a comprehensive overview of all available builtin commands, matching the functionality users expect from modern shells like Fish.
