# Lyra Shell

Lyra is the default command-line shell of SOL, providing an intelligent, consistent, and elegant command-line experience.

## Current Status

✅ **Phase 2 (Intelligence) complete**
✅ **Phase 3 (Core Commands) complete** - 31 builtin commands

Implemented features:

### Core (Phase 1)
- ✅ Lexer - implemented with logos
- ✅ Parser - full expression and statement parsing
- ✅ Runtime evaluator - supports variables, expressions, and pipelines
- ✅ Basic built-in commands:
  - `echo` - print text
  - `ls` - list files (supports `--all`, `--long` flags)
  - `cd` - change directory
  - `pwd` - print the current directory
  - `exit` - exit the shell
  - `which` - find command location
  - `clear` - clear the screen
  - `reset` - reset the terminal
- ✅ **File operations** (Phase 3):
  - `cat` - display file contents
  - `cp` - copy files and directories
  - `mv` - move/rename files
  - `rm` - remove files or directories
  - `mkdir` - create directories
  - `touch` - create/update files
- ✅ **Text utilities** (Phase 3):
  - `grep` - search text patterns
  - `head` - show first lines
  - `tail` - show last lines
  - `wc` - count words/lines/characters
  - `sort` - sort lines
  - `uniq` - remove duplicate lines
- ✅ **System utilities** (Phase 3):
  - `env` - display environment variables
  - `basename` - strip directory from path
  - `dirname` - get directory from path
  - `sleep` - delay execution
  - `date` - show date/time
  - `true` / `false` - return success/failure
  - `whoami` - show current user
  - `uname` - show system information
- ✅ External command execution - can run system commands
- ✅ Pipeline support - `cmd1 | cmd2 | cmd3`
- ✅ Variable system - `let x = 42`, `$x`
- ✅ Control flow - `if`, `for`, `while`
- ✅ Structured data - List, Record, Table
- ✅ Table rendering - nicely formatted table output
- ✅ Simple prompt - `λ ~/path (git-branch)`
- ✅ Reedline integration - a modern readline experience

### Intelligence Features (Phase 2)
- ✅ **Intelligent completion engine**
  - ✅ Command completion (built-ins + PATH commands)
  - ✅ File path completion with size display
  - ✅ Git-aware completion (branches, remotes, subcommands)
  - ✅ Context-aware routing
- ✅ **Syntax highlighting**
  - ✅ Command highlighting (built-ins in blue, external in yellow)
  - ✅ String highlighting (green)
  - ✅ Variable highlighting (cyan)
  - ✅ Operator highlighting (magenta)
  - ✅ Flag highlighting (cyan)
  - ✅ Number highlighting (magenta)
- ✅ **History management**
  - ✅ Persistent history with metadata
  - ✅ Search functionality
  - ✅ Timestamp and working directory tracking
  - ✅ Exit status tracking
  - ✅ Reedline history integration (Ctrl+R search)
- ✅ Test coverage - 25 unit tests passing

## Quick Start

```bash
# Build
cargo build -p lyra

# Run
cargo run -p lyra

# Test
cargo test -p lyra
```

## Examples

```bash
# Basic commands with syntax highlighting
λ echo "Hello, SOL!"
Hello, SOL!

λ pwd
/home/user/Projects/SOL

λ cd lyra

# Tab completion for commands, files, and git
λ ec<TAB>          # Completes to 'echo'
λ ls src/co<TAB>   # Completes to 'src/completion/'
λ git che<TAB>     # Completes to 'git checkout'

# Variables
λ let name = "Lyra"
λ echo $name
Lyra

# Control flow
λ if true { echo "yes" }
yes

λ for x in [1, 2, 3] { echo $x }
1
2
3

# History search (Ctrl+R)
# Type Ctrl+R and start typing to search command history
```

## Architecture

```
lyra/
├── src/
│   ├── lexer/          # Lexical analysis
│   │   ├── token.rs    # Token definitions
│   │   └── mod.rs      # Scanner
│   ├── parser/         # Syntax analysis
│   │   ├── ast.rs      # AST definitions
│   │   ├── error.rs    # Parse errors
│   │   └── mod.rs      # Parser
│   ├── runtime/        # Runtime
│   │   ├── eval.rs     # Evaluator
│   │   ├── env.rs      # Environment/scope
│   │   ├── error.rs    # Runtime errors
│   │   └── mod.rs
│   ├── builtins/       # Built-in commands
│   │   ├── basic.rs    # Basic commands
│   │   ├── external.rs # External commands
│   │   ├── registry.rs # Command registry
│   │   └── mod.rs
│   ├── completion/     # Intelligent completion
│   │   ├── completer.rs # Main completer
│   │   ├── command.rs  # Command completion
│   │   ├── file.rs     # File path completion
│   │   ├── git.rs      # Git-aware completion
│   │   └── mod.rs
│   ├── highlighter/    # Syntax highlighting
│   │   └── mod.rs
│   ├── history/        # History management
│   │   ├── manager.rs  # History manager
│   │   └── mod.rs
│   ├── prompt/         # Prompt
│   │   └── mod.rs
│   ├── lib.rs          # Library interface
│   └── main.rs         # Entry point
├── Cargo.toml
└── README.md
```

## Next Steps (Phase 4)

- [ ] Additional busybox commands
  - [ ] `find` - find files by pattern
  - [ ] `xargs` - build command lines
  - [ ] `cut` - extract columns
  - [ ] `tr` - translate characters
  - [ ] `tee` - split output
  - [ ] `ln` - create links
  - [ ] `chmod` / `chown` - permissions
  - [ ] `df` / `du` - disk usage
  - [ ] `stat` - file information
- [ ] Configuration system
  - [ ] Custom prompt templates
  - [ ] Color themes
  - [ ] Keybindings
- [ ] Advanced features
  - [ ] Functions and modules
  - [ ] Plugin system
  - [ ] SOL system integration

## Test Results

```bash
running 25 tests
test completion::command::tests::test_builtin_commands ... ok
test completion::command::tests::test_discover_path_commands ... ok
test completion::completer::tests::test_completion_context_command ... ok
test completion::completer::tests::test_completion_context_git ... ok
test completion::completer::tests::test_completion_context_path ... ok
test completion::file::tests::test_format_file_size ... ok
test completion::file::tests::test_parse_partial_path_empty ... ok
test completion::file::tests::test_parse_partial_path_relative ... ok
test completion::git::tests::test_git_subcommands ... ok
test highlighter::tests::test_highlight_builtin ... ok
test highlighter::tests::test_highlight_simple_command ... ok
test history::manager::tests::test_history_entry_creation ... ok
test history::manager::tests::test_history_search ... ok
test history::manager::tests::test_recent_entries ... ok
test lexer::tests::test_tokenize_number ... ok
test lexer::tests::test_tokenize_pipeline ... ok
test lexer::tests::test_tokenize_simple_command ... ok
test lexer::tests::test_tokenize_string ... ok
test parser::tests::test_parse_binary_expr ... ok
test parser::tests::test_parse_let ... ok
test parser::tests::test_parse_pipeline ... ok
test parser::tests::test_parse_simple_command ... ok
test runtime::eval::tests::test_eval_binary_op ... ok
test runtime::eval::tests::test_eval_let ... ok
test runtime::eval::tests::test_eval_literal ... ok

test result: ok. 25 passed; 0 failed; 0 ignored
```

## Design Documentation

For detailed design documents, see:
- [Architecture](../docs/lyra/architecture.md)
- [Syntax Design](../docs/lyra/syntax.md)
- [Intelligence Features](../docs/lyra/intelligence.md)
- [Data Model](../docs/lyra/data-model.md)

## License

Same license as the SOL project (BSD-3-Clause).
