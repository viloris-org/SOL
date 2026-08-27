# Lyra Shell

Lyra is the default command-line shell of SOL, providing an intelligent, consistent, and elegant command-line experience.

## Current Status

✅ **Phase 1 (MVP) complete**

Implemented features:

- ✅ Lexer - implemented with logos
- ✅ Parser - full expression and statement parsing
- ✅ Runtime evaluator - supports variables, expressions, and pipelines
- ✅ Basic built-in commands:
  - `echo` - print text
  - `ls` - list files (supports `--all`, `--long` flags)
  - `cd` - change directory
  - `pwd` - print the current directory
  - `exit` - exit the shell
- ✅ External command execution - can run system commands
- ✅ Pipeline support - `cmd1 | cmd2 | cmd3`
- ✅ Variable system - `let x = 42`, `$x`
- ✅ Control flow - `if`, `for`, `while`
- ✅ Structured data - List, Record, Table
- ✅ Table rendering - nicely formatted table output
- ✅ Simple prompt - `λ ~/path (git-branch)`
- ✅ Reedline integration - a modern readline experience
- ✅ Test coverage - 11 unit tests passing

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
# Basic commands
λ echo "Hello, SOL!"
Hello, SOL!

λ pwd
/home/user/Projects/SOL

λ cd lyra

# Variables
λ let name = "Lyra"
λ echo $name
Lyra

# Pipelines (coming soon)
λ ls --long | where size > 1000

# Control flow
λ if true { echo "yes" }
yes

λ for x in [1, 2, 3] { echo $x }
1
2
3
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
│   ├── prompt/         # Prompt
│   │   └── mod.rs
│   ├── lib.rs          # Library interface
│   └── main.rs         # Entry point
├── Cargo.toml
└── README.md
```

## Next Steps (Phase 2)

- [ ] Intelligent completion engine
  - [ ] File path completion
  - [ ] Command completion
  - [ ] Git completion
- [ ] Syntax highlighting
- [ ] History management and search
- [ ] More built-in commands (grep, cat, where, sort-by, select)

## Test Results

```bash
running 11 tests
test lexer::tests::test_tokenize_number ... ok
test lexer::tests::test_tokenize_simple_command ... ok
test lexer::tests::test_tokenize_pipeline ... ok
test parser::tests::test_parse_let ... ok
test lexer::tests::test_tokenize_string ... ok
test parser::tests::test_parse_simple_command ... ok
test parser::tests::test_parse_pipeline ... ok
test parser::tests::test_parse_binary_expr ... ok
test runtime::eval::tests::test_eval_literal ... ok
test runtime::eval::tests::test_eval_binary_op ... ok
test runtime::eval::tests::test_eval_let ... ok

test result: ok. 11 passed; 0 failed; 0 ignored
```

## Design Documentation

For detailed design documents, see:
- [Architecture](../docs/lyra/architecture.md)
- [Syntax Design](../docs/lyra/syntax.md)
- [Intelligence Features](../docs/lyra/intelligence.md)
- [Data Model](../docs/lyra/data-model.md)

## License

Same license as the SOL project (BSD-3-Clause).
