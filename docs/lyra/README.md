# Lyra Shell

Lyra is the **default command-line shell** of SOL, providing an intelligent, consistent, and elegant command-line experience.

As the native shell of the SOL operating system, Lyra is deeply integrated with the system from day one and serves as the standard way for users to interact with it.

## Design Philosophy

Lyra treats commands as functions and pipelines as data flow. Unlike the text streams of traditional shells, Lyra's pipelines carry structured data (tables, records, lists), making command composition more type-safe and predictable.

### Core Principles

1. **Consistency over compatibility** - Unified syntax rules, no historical baggage
2. **Intelligence over brevity** - Context-aware completion, error correction, and preview
3. **Type safety** - Structured data pipelines with compile-time type checking
4. **Elegant syntax** - A human-friendly way to write commands
5. **Platform integration** - Deep integration with the SOL system

## Contents

- [Syntax Design](./syntax.md) - Language rules and examples
- [Intelligence Features](./intelligence.md) - Completion, error correction, preview
- [Prompt Configuration](./prompt.md) - Prompt design and customization
- [Data Model](./data-model.md) - Structured data and pipelines
- [Built-in Commands](./builtins.md) - Core command reference
- [Plugin System](./plugins.md) - Extending Lyra
- [Architecture](./architecture.md) - Technical implementation

## Quick Start

Lyra is the default shell of SOL and works out of the box:

```bash
# Open a terminal and Lyra starts automatically
λ echo "Hello, SOL!"
Hello, SOL!

# Lyra is preinstalled, no installation needed
λ which lyra
/usr/bin/lyra (builtin)

# Used by default in sol-terminal
# Used by default on TTY login
# Used by default in system scripts: #!/usr/bin/lyra
```

### First Commands

```rust
λ echo "Hello, SOL!"
Hello, SOL!

λ ls | where size > 1MB | sort-by modified
╭───┬─────────────┬──────────┬─────────────────────╮
│ # │    name     │   size   │      modified       │
├───┼─────────────┼──────────┼─────────────────────┤
│ 0 │ video.mp4   │ 45.2 MB  │ 2024-01-15 14:30:22 │
│ 1 │ dataset.db  │  8.7 MB  │ 2024-01-15 09:15:03 │
╰───┴─────────────┴──────────┴─────────────────────╯

λ git log --oneline -5 | parse "{hash} {message}"
╭───┬──────────┬─────────────────────────────╮
│ # │   hash   │           message           │
├───┼──────────┼─────────────────────────────┤
│ 0 │ a3f8b92  │ Add lyra documentation      │
│ 1 │ 7c2d1e5  │ Implement prompt system     │
│ 2 │ 9f4a8b3  │ Add structured data model   │
╰───┴──────────┴─────────────────────────────╯
```

## Key Differences

### vs Bash/Zsh

| Feature         | Bash/Zsh        | Lyra                            |
|-----------------|-----------------|---------------------------------|
| Data flow       | Text stream     | Structured data (tables/records)|
| Variable syntax | `$var` `${var}` | Unified `$var` `${expr}`        |
| Pipe semantics  | Text concatenation | Type-aware data transformation |
| Error handling  | `$?` exit code  | First-class `$error`            |
| Configuration   | Shell scripts   | Declarative TOML configuration  |

### vs Fish

| Feature         | Fish            | Lyra                            |
|-----------------|-----------------|---------------------------------|
| Syntax          | Close to Bash   | More modern (Rust-style)        |
| Data model      | Text/lists      | Full structured type system     |
| Plugin system   | Fish functions  | Rust module system              |
| Platform integration | Generic    | Native SOL integration          |

### vs Nushell

| Feature         | Nushell         | Lyra                            |
|-----------------|-----------------|---------------------------------|
| Philosophy      | Structured shell| Same (inspired by Nu)           |
| Syntax          | Custom          | Closer to traditional (lower learning curve) |
| Integration     | Cross-platform generic | Deep SOL integration     |
| Goal            | Standalone tool | Part of the SOL platform        |

## Development Status

**Current stage: design and planning**

Lyra is currently in the architecture design stage. As the default shell of SOL, its development is on the critical path for the system release.

> **Note**: Lyra is not an optional component; it is a core part of the SOL system. By the SOL 1.0 release, Lyra must reach production quality.

Here is the development roadmap:

### Phase 1: Foundations (MVP)
- [ ] Syntax parser (variables, pipelines, command invocation)
- [ ] Basic REPL (reedline integration)
- [ ] Built-in commands (cd, ls, echo, exit)
- [ ] Simple prompt
- [ ] Structured output table rendering

### Phase 2: Intelligence Features
- [ ] Intelligent completion engine
- [ ] Command history and search
- [ ] Syntax highlighting
- [ ] Error hints and correction
- [ ] Inline preview

### Phase 3: Advanced Features
- [ ] Plugin system
- [ ] Function definitions and modules
- [ ] Configuration system
- [ ] Theme support
- [ ] SOL system integration

### Phase 4: Polish
- [ ] Full built-in command library
- [ ] External command adapter improvements
- [ ] Performance optimization
- [ ] Documentation and examples
- [ ] Test coverage

## Contributing

Lyra is a core component of the SOL project. Design docs live in `docs/lyra/`, and the implementation lives in the `lyra/` crate.

As the system's default shell, Lyra is a high-priority workstream. Design suggestions and implementation contributions are welcome.

### Development Priorities

Because Lyra is the default shell, its development must be completed in the early stages of SOL:

- **Phase 1 (MVP)**: before the SOL Alpha release
- **Phase 2 (intelligence features)**: before the SOL Beta release
- **Phase 3 (advanced features)**: before the SOL 1.0 release
- **Phase 4 (polish)**: ongoing improvement after SOL 1.0

## License

Same license as the SOL project.
