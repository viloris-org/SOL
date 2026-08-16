# SOL Diagnostics

`sol-diagnostics` is the local, typed diagnostics foundation for SOL services,
the shell, compositor, and first-party applications. It is intentionally a
storage and API boundary, not a telemetry client or a system crash-handler.

## Current contract

- Every record has an allowlisted `DiagnosticCode`, severity, timestamp, and
  typed `DiagnosticSource` (`SolComponent` or validated `AppId`). Live caller
  authentication remains a separate transport concern.
- There is no API for command lines, environment snapshots, process arguments,
  stack traces, arbitrary key/value payloads, attachments, or upload.
- Optional summaries pass through deterministic credential and home-path
  redaction, control-character removal, and a 240-character cap before they
  can reach either store.
- `DiagnosticsService` retains only the configured newest records (256 by
  default) and writes the bounded snapshot through `DiagnosticStore`.
- `MemoryDiagnosticStore` is appropriate for tests; `FileDiagnosticStore`
  atomically replaces its daemon-private versioned file and applies mode 0600
  on Unix.

The executable initializes the local store at
`$XDG_STATE_HOME/sol/diagnostics.log`, falling back to
`$HOME/.local/state/sol/diagnostics.log`. It does not collect, upload, or
report a crash by itself.

```bash
cargo test -p sol-diagnostics
cargo run -p sol-diagnostics
```

## Deferred work

Running-service transport, trusted source authentication, crash capture,
consent UX, encrypted export/upload, upload policy, and field validation are
separate decisions. They must consume this bounded schema rather than adding a
shell or opaque-payload escape hatch.

See [ADR-0016](../../docs/decisions/0016-diagnostics-foundation.md).
