# Tab Completion Fixes

## Issues Identified

From the screenshot, three main problems were identified:

1. **Wrong completion context**: Typing `ls ` (with space) was showing commands instead of files/directories
2. **Vertical layout**: Completions were stacking vertically instead of flowing horizontally in columns
3. **No tab navigation**: Tab wasn't cycling through completions properly

## Root Causes

### Issue 1: Context Detection Bug

**Location**: `lyra/src/completion/completer.rs:25-47`

The `get_completion_context()` function was splitting by whitespace and checking `tokens.len() > 1` to determine if we should complete paths. However, when you type `ls ` (command + space), `split_whitespace()` returns `["ls"]` (one token), so it was staying in `Command` context.

**Fix**: Added explicit check for trailing whitespace:

```rust
// If line ends with whitespace, we're starting a new argument (path completion)
if before_cursor.ends_with(char::is_whitespace) {
    return CompletionContext::Path;
}
```

### Issue 2: Path Extraction Bug

**Location**: `lyra/src/completion/file.rs:16-36`

The `FileCompleter::complete()` was using `split_whitespace()` and taking the last token. With `ls ` (trailing space), this would return `"ls"` as the partial path, which is wrong.

**Fix**: Changed to extract everything after the last whitespace character:

```rust
let partial = if let Some(last_space) = before_cursor.rfind(char::is_whitespace) {
    &before_cursor[last_space + 1..]
} else {
    before_cursor
};
```

Now `ls ` correctly gives an empty partial `""`, which lists all files in the current directory.

### Issue 3: Menu Configuration

**Location**: `lyra/src/lib.rs:53-68`

The menu needed:
- Fixed column width for proper horizontal layout
- `UntilFound` event sequence for tab navigation
- Shift+Tab binding for reverse navigation

**Fix**: 

```rust
let completion_menu = Box::new(
    ColumnarMenu::default()
        .with_name("completion_menu")
        .with_columns(4)
        .with_column_width(Some(20))  // Fixed width prevents vertical stacking
        .with_column_padding(2)
        .with_text_style(nu_ansi_term::Style::default().dimmed()),
);

// Tab opens menu or moves to next completion
keybindings.add_binding(
    KeyModifiers::NONE,
    KeyCode::Tab,
    ReedlineEvent::UntilFound(vec![
        ReedlineEvent::Menu("completion_menu".to_string()),
        ReedlineEvent::MenuNext,
    ]),
);

// Shift+Tab moves to previous completion
keybindings.add_binding(
    KeyModifiers::SHIFT,
    KeyCode::BackTab,
    ReedlineEvent::MenuPrevious,
);
```

## Expected Behavior After Fixes

1. **`ls <Tab>`** → Shows files and directories in current folder
   - Directories have `/` suffix and "directory" description
   - Files show size (e.g., "1.2 KB")
   - Layout: 4 columns, horizontal flow, wraps when needed

2. **Tab Navigation**:
   - First Tab: Opens completion menu
   - Subsequent Tabs: Cycles forward through completions
   - Shift+Tab: Cycles backward through completions

3. **Context Switching**:
   - `ls` → command completions
   - `ls ` → file/path completions
   - `cd ` → file/path completions (directories prioritized)
   - `git ` → git subcommands

## Testing

Run the shell and test:

```bash
cd lyra
cargo run

# In Lyra shell:
ls <Tab>        # Should show files/dirs in columns
cd <Tab>        # Should show directories with /
echo <Tab>      # Should show files/dirs (any command + space)
git<Tab>        # Should show 'git' command
git <Tab>       # Should show git subcommands
```

## Files Modified

1. `lyra/src/completion/completer.rs` - Fixed context detection
2. `lyra/src/completion/file.rs` - Fixed path extraction
3. `lyra/src/lib.rs` - Improved menu configuration and keybindings
