# sol-app

SOL application framework: application lifecycle, window lifecycle, session
restoration, commands, menus, keyboard shortcuts, clipboard, drag & drop,
notifications, recent documents, state restoration. (PRD §20.)

## Architecture

```text
Application
  Scene
  Window
  View
  Command
  Document
  Task
  SystemService
```

The framework should make correct behavior the default rather than asking
every developer to hand-replicate system behavior (PRD §4.3 Framework First).

## Status

**Phase 0 scaffold.** The API is designed in Phase 2 and dogfooded alongside
`sol-files` / `sol-terminal` / `sol-settings`.
