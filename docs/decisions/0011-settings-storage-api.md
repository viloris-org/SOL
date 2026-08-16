# 11. Settings storage and stable minimum API boundary

- **Status:** **Accepted** ✅ (Phase 2 M2)
- **Date:** 2026-08-15
- **Decision date:** 2026-08-15
- **Target phase:** Phase 2, consumed by Phase 3 Settings

## Context

PRD §26 requires a strict `Settings UI → Settings API → system services`
layering.  The Phase 0 `sol-system` and `sol-settingsd` crates were scaffolds,
so a Settings application would otherwise have to choose a file format, a
daemon protocol, or ad-hoc string keys on its own.  That would entangle the UI
with implementation details before Appearance and Sound can be dogfooded in
Phase 3.

## Decision

`sol-system` owns the small, typed client-facing contract:

- `SettingsApi` exposes `snapshot()` and `apply(SettingsChange)`;
- `SettingsSnapshot` is a coherent view with a monotonically increasing
  revision;
- `SettingsChange` has explicit typed variants, beginning with colour scheme,
  output volume (a validated 0–100 `OutputVolume`), and output mute;
- `SettingsError` is the single client-visible failure type.

The API intentionally contains no storage paths, serialization, D-Bus types,
or free-form string key/value interface.  New settings pages add named domain
types and explicit change variants; they do not establish a second protocol.

`sol-settingsd` owns the implementation boundary through `SettingsStore` and
the `org.sol.Settings1` session-bus adapter. The adapter maps only complete
revisioned snapshots and explicit `SettingsChange` variants; it never exposes a
file path, a backend object, or free-form setting keys. `SettingsDbusProxy`
implements the same `SettingsApi` consumed by first-party UI clients.
`SettingsDaemon<S>` validates and revisions a snapshot, then writes it through
the store before publishing it to readers.  It currently supplies:

- `MemorySettingsStore` for tests and embedded development;
- `FileSettingsStore` for user persistence.  It uses a versioned,
  line-oriented daemon-private format and atomically replaces the target file
  after syncing a temporary file.

The default daemon path is `$XDG_CONFIG_HOME/sol/settings.conf`, falling back
to `$HOME/.config/sol/settings.conf`.  The file format is deliberately not a
SolKit contract: a later migration to a database or another system settings
backend changes only a `SettingsStore` implementation.

## Consequences

- A Phase 3 Settings UI can be written and mock-tested solely against
  `SettingsApi`, then use `SettingsDbusProxy` without changing its domain code.
- IPC remains an adapter concern. `org.sol.Settings1` delegates to the same
  daemon core in line with ADR-0006, without exposing D-Bus in `sol-system`.
- The first stable surface is intentionally narrow.  Network, displays,
  Bluetooth, input, power, and accessibility settings must add typed domains
  as their backing services become real.
- Files written by the current daemon are forward-compatible at the parser
  level for unknown fields, but are not a public configuration-file interface.

## Rejected options

1. **Let the Settings UI read and write a config file directly.** Rejected:
   it violates PRD §26 and would duplicate persistence and privilege policy in
   each UI surface, including Quick Settings.
2. **Expose `get("key")` / `set("key", value)` as the API.** Rejected:
   types, validation, discovery, and migration rules would be deferred into
   every caller.  Explicit change variants make the initial API small and
   reviewable.
3. **Make a JSON/TOML file the public API.** Rejected: it would freeze a
   persistence format before the system-service model and permissions exist.
4. **Expose generic D-Bus properties or a string map.** Rejected: the session
   adapter preserves the same complete snapshots and explicit mutations as the
   in-process API, so no new untyped settings protocol is introduced.

## Verification

`sol-system` contains a mock `SettingsApi` round-trip test, and
`sol-settingsd` tests both service-to-memory-store write-through and a
file-store reload round-trip. `scripts/validate-settingsd-dbus.sh` starts the
real daemon under `dbus-run-session`, applies settings through `busctl` and
`SettingsDbusProxy`, and reads the typed result back through the session bus.
