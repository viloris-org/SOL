# 12. Application identity and lifecycle contracts

- **Status:** **Accepted** ✅ (Phase 2 M2)
- **Date:** 2026-08-16
- **Decision date:** 2026-08-16
- **Target phase:** Phase 2; consumed by Phase 3 apps and Phase 4 shell/services

## Context

PRD §20 gives SolApp ownership of application lifecycle, while PRD §21 and
§28 require commands and launcher/search surfaces to refer to applications
consistently. PRD §41 decision gate #7 explicitly requires an application
identity format before the launcher, notifications, command system, and store
grow their own incompatible identifiers.

The original `AppId` scaffold combined an unchecked reverse-DNS string with a
version. That made an app's durable identity ambiguous with its current package
release, and supplied no parse boundary for shell or service callers.

## Decision

`sol-app` owns the small, typed identity and process-lifecycle contract.

### Application ID

`AppId` is an opaque, validated reverse-DNS identifier with this canonical
ASCII grammar:

```text
app-id    = component "." component *( "." component )
component = lower-alpha *( lower-alpha / digit / "-" ) ( lower-alpha / digit )
lower-alpha = "a" … "z"
digit       = "0" … "9"
```

It has at least two components and at most 255 UTF-8 bytes. Examples include
`org.sol.files` and `io.example.photo-editor`. `AppId::parse`, `FromStr`, and
`TryFrom` are the only construction paths; callers serialize it with
`as_str()` / `Display`.

An app ID is never a package name, installed path, desktop-file filename,
D-Bus name, URI scheme, or release version. Package managers and the future
store may attach versions and distribution metadata to an `AppId`, but a
release update must not change the ID. This keeps saved command references,
notification attribution, launcher pins, and store records stable across
updates.

`AppIdentity` adds only the shared, non-empty display name needed by launcher
and notification UI. Icon transport/asset representation, package metadata,
and store catalogue fields are deliberately deferred to their respective
layers so this gate does not freeze an image or distribution API.

### Consumer boundary

`AppId` is the required owner/foreign key for all cross-process application
references:

- **Launcher/search:** launch targets, pins, and result grouping use `AppId`;
  user-visible labels use `AppIdentity`.
- **Notifications:** attribution records the emitting `AppId`, with optional
  `AppIdentity` display metadata supplied by the UI layer.
- **Commands:** a command remains identified by its existing semantic command
  ID (for example `file.open`), and its application owner is carried as an
  `AppId`; a command ID alone is not an app identity.
- **Store/package integration:** catalogue and installed-release records are
  keyed by `AppId`; package/repository/version data stays outside `sol-app`.

Adapters to D-Bus, desktop files, package metadata, or a future store resolve
their external names at the boundary and validate them before producing an
`AppId`. They do not expose raw strings as a second SolKit identity type.

### Lifecycle boundary

`AppLifecycle` is the process-local state machine used by `App`:

```text
Starting --start--> Running --suspend--> Suspended --resume--> Running
    |                  |                    |
    +------------------+--------------------+--stop--> Stopped
```

`Stopped` is terminal for an instance; launching again creates a fresh
`AppLifecycle` in `Starting`. Invalid transitions return `LifecycleError`
instead of mutating state. Window focus is an input to suspend/resume policy,
not an independent lifecycle state, and a window does not own the process
lifecycle. Process supervision, session restoration, and IPC event delivery
will adapt this state machine later rather than adding transitions directly.

## Consequences

- App IDs are validated at application-manifest/adapter ingress and become
  safe, comparable, hashable values through the framework.
- `App` no longer exposes mutable lifecycle state; callers use checked
  lifecycle methods and inspect `state()`.
- `AppId` no longer carries a version. Existing scaffold consumers must parse
  the ID first and keep release metadata in their package/store layer.
- The contract is intentionally narrow: sandbox identity, desktop-file
  discovery, command catalogue transport, notification delivery, and store
  backend remain separate decisions and adapters.

## Rejected options

1. **Use package name plus version as the universal identity.** Rejected:
   repository names and release versions can change independently of a user's
   notion of the application, breaking pins and attribution.
2. **Accept arbitrary strings and validate only in each service.** Rejected:
   this recreates parsing rules in the launcher, notifications, commands, and
   store while allowing invalid values to travel through the SDK.
3. **Make a desktop-file ID, D-Bus name, or URI scheme canonical.** Rejected:
   each has different character, ownership, and lifecycle rules, and none
   should force the others' transport constraints into the SolKit contract.
4. **Treat window focus as process suspension.** Rejected: a multi-window app
   may stay live without one focused window; lifecycle is process-scoped and
   policy adapters decide when inactive means suspended.

## Verification

`sol-app` unit tests cover canonical parsing, every grammar boundary, maximum
length, display-name validation, the allowed lifecycle path, invalid
transitions, and the terminal stopped state. The crate also tests that `App`
uses the checked lifecycle contract.

## Related

- PRD §20 (SolApp), §21 (Command architecture), §28 (Search & launcher), §41
  decision #7
- ADR-0006 (shell/service IPC boundary)
