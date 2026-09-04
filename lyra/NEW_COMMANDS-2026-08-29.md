# New Built-in Commands - 2026-08-29

## Overview

Added three commonly-used shell commands to Lyra's built-in command set:

- `which` - Find the location of a command
- `clear` - Clear the terminal screen
- `reset` - Reset the terminal to initial state

## Commands

### `which`

Locates where a command is found in the system.

**Usage:**
```bash
which <command>
```

**Examples:**
```bash
λ ~ 〉which cargo
/home/user/.cargo/bin/cargo

λ ~ 〉which echo
echo: shell built-in command

λ ~ 〉which nonexistent
nonexistent not found
```

**Behavior:**
- Checks if the command is a built-in first
- Searches through PATH directories for executables
- Returns the full path if found
- Shows an error message if not found

### `clear`

Clears the terminal screen and moves the cursor to the top-left.

**Usage:**
```bash
clear
```

**Implementation:**
- Uses ANSI escape sequence `\x1B[2J\x1B[1;1H`
- Clears screen without resetting terminal state
- Fast and lightweight

**Example:**
```bash
λ ~ 〉ls
file1.txt  file2.txt  dir1/

λ ~ 〉clear
# Screen is now clear

λ ~ 〉
```

### `reset`

Resets the terminal to its initial state.

**Usage:**
```bash
reset
```

**Implementation:**
- Uses ANSI escape sequence `\x1Bc` (ESC c)
- Performs a full terminal reset
- Clears screen, resets colors, fonts, and terminal modes
- More comprehensive than `clear`

**Example:**
```bash
λ ~ 〉# Terminal is messed up with weird colors or characters
λ ~ 〉reset
# Terminal is now back to initial state

λ ~ 〉
```

**When to use:**
- Use `clear` for normal screen clearing (faster)
- Use `reset` when terminal rendering is corrupted or behaving strangely

## Implementation Details

### Files Modified

1. **`lyra/src/builtins/basic.rs`**
   - Added `Which`, `Clear`, and `Reset` structs
   - Each implements the `Builtin` trait with `async execute()`

2. **`lyra/src/builtins/mod.rs`**
   - Exported new command structs

3. **`lyra/src/builtins/registry.rs`**
   - Registered new commands in `BuiltinRegistry::new()`

4. **`lyra/src/completion/command.rs`**
   - Added new commands to builtin completion list

5. **`lyra/src/highlighter/mod.rs`**
   - Added new commands to syntax highlighting

### Technical Implementation

#### `which` Command
```rust
pub struct Which;

#[async_trait]
impl Builtin for Which {
    fn name(&self) -> &str {
        "which"
    }

    async fn execute(&self, args: Vec<Value>, _flags: HashMap<String, Value>) 
        -> RuntimeResult<Value> {
        // 1. Check builtins
        // 2. Search PATH directories
        // 3. Return path or error
    }
}
```

#### `clear` Command
```rust
pub struct Clear;

#[async_trait]
impl Builtin for Clear {
    fn name(&self) -> &str {
        "clear"
    }

    async fn execute(&self, _args: Vec<Value>, _flags: HashMap<String, Value>) 
        -> RuntimeResult<Value> {
        print!("\x1B[2J\x1B[1;1H");
        std::io::stdout().flush()?;
        Ok(Value::Null)
    }
}
```

#### `reset` Command
```rust
pub struct Reset;

#[async_trait]
impl Builtin for Reset {
    fn name(&self) -> &str {
        "reset"
    }

    async fn execute(&self, _args: Vec<Value>, _flags: HashMap<String, Value>) 
        -> RuntimeResult<Value> {
        print!("\x1Bc");
        std::io::stdout().flush()?;
        Ok(Value::Null)
    }
}
```

## Testing

Added integration tests in `lyra/tests/test_new_commands.rs`:

```rust
#[tokio::test]
async fn test_which_command_parsing() { ... }

#[tokio::test]
async fn test_clear_command_parsing() { ... }

#[tokio::test]
async fn test_reset_command_parsing() { ... }
```

**Test Results:**
- ✅ All 3 new tests pass
- ✅ All 25 existing unit tests pass
- ✅ All 3 existing integration tests pass
- ✅ **Total: 31 tests passing**

## Integration with Intelligence Features

### Tab Completion
All three commands now support tab completion:

```bash
λ ~ 〉wh<TAB>
which

λ ~ 〉cl<TAB>
clear

λ ~ 〉re<TAB>
reset
```

### Syntax Highlighting
Commands are highlighted in blue (built-in command color):

```bash
λ ~ 〉which cargo
      ^^^^^
      blue (built-in)
```

### History
Commands are tracked in shell history with metadata:

```bash
λ ~ 〉<Ctrl+R>
# Search: clear
# Shows: clear (timestamp: 2026-08-29 10:30:45, cwd: /home/user)
```

## Usage Statistics

**Built-in Commands Now Available:**
- Phase 1 original: `echo`, `ls`, `cd`, `pwd`, `exit` (5 commands)
- Phase 1 enhanced: +`which`, `clear`, `reset` (8 commands)

**Total Command Set:**
- 8 built-in commands
- Unlimited external commands via PATH
- 5+ control flow keywords (`let`, `if`, `for`, `while`, etc.)

## Comparison with Other Shells

| Command | Bash | Fish | Zsh | Lyra |
|---------|------|------|-----|------|
| `which` | External | Built-in | External | Built-in |
| `clear` | External | Built-in | External | Built-in |
| `reset` | External | N/A | External | Built-in |

Lyra makes these commands built-in for:
- **Faster execution** (no process spawning)
- **Better portability** (no external dependencies)
- **Consistent behavior** (same on all platforms)

## Future Enhancements

Potential improvements for Phase 3:

- `which -a` flag to show all matches in PATH
- `which -s` for silent mode (exit code only)
- `clear -x` to clear scrollback buffer
- Tab completion for `which <command>` arguments

## Summary

✅ **Status:** Complete and tested
✅ **Commands Added:** 3 (`which`, `clear`, `reset`)
✅ **Tests Added:** 3 integration tests
✅ **Total Tests:** 31 (all passing)
✅ **Documentation:** Updated README and IMPLEMENTATION_SUMMARY

These commands improve Lyra's usability and bring it closer to feature parity with modern shells like Fish and Zsh.
