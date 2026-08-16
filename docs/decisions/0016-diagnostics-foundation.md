# ADR-0016: Privacy-bounded diagnostics foundation

- **Status:** Accepted (Phase 5 foundation)
- **Date:** 2026-08-16
- **Decision:** Establish a typed, source-attributed local diagnostics service
  with deterministic redaction and bounded retention. It must not offer shell
  execution, arbitrary payload capture, or implicit upload.

## Context

PRD §41 decision #18 needs a crash-reporting and diagnostics architecture,
but early collection often turns logging into an unchecked store of commands,
environment variables, file paths, personal data, and opaque crash dumps.
That would conflict with SOL's typed service boundaries and make consent and
retention difficult to audit.

## Decision

`sol-diagnostics` owns the first typed local boundary. A `DiagnosticEvent`
contains only a typed `DiagnosticSource`, `DiagnosticSeverity`, allowlisted
`DiagnosticCode`, and an optional `RedactedDiagnosticText`. Sources are either
a closed `SolComponent` catalog or a validated `sol-app::AppId`; callers cannot
claim an arbitrary string source.

The event model deliberately has no fields or methods for shell commands,
arguments, environment variables, current directories, stack traces, arbitrary
key/value maps, byte payloads, attachments, or network destinations. Optional
summaries remove control characters, redact common credential forms and
home-directory paths, and are limited to 240 characters. This filtering is a
defense in depth measure, not consent for broad data collection.

`DiagnosticsService` assigns timestamps and monotonic sequence numbers, trims
oldest records to an explicit `DiagnosticRetention` ceiling, and persists the
complete bounded snapshot before publishing it in memory. Both memory and
versioned file stores use the same typed snapshot. The file store atomically
replaces its private format and restricts the final file to mode 0600 on Unix.

## Consequences

- Component and application failures now have a constrained, testable local
  reporting contract without introducing a telemetry dependency.
- Unit tests prove source attribution, redaction, bounded in-memory retention,
  write-through persistence, and a file-store reload; they do not prove crash
  capture from a running desktop session.
- Diagnostics consumers must map failures to the closed code catalog. New
  categories require an API review instead of a free-form event-name escape
  hatch.

## Deferred work and non-claims

This foundation does not authenticate a live transport caller, capture a real
process crash, collect a backtrace, provide a consent UI, encrypt records,
upload anything, or establish remote telemetry retention. Those capabilities
need separate threat-model, consent, and operational decisions. In particular,
adding arbitrary shell access or opaque crash payloads would violate this ADR.

## Related

- PRD §4, §31, §41 decision #18
- ADR-0013 (typed System Action and permission layer)
- [Service README](../../services/sol-diagnostics/README.md)
