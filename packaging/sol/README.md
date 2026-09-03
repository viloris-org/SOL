# SOL native packaging

This directory is reserved for the Phase 8–9 native package work defined by
[ADR-0020](../../docs/decisions/0020-sol-package-app-runtime.md).

Ownership:

```text
packaging/sol/
├── bundle/         implemented .app content manifest, signing, lineage, verification CLI
├── client/         sol-pkg CLI and unprivileged API client
├── daemon/         sol-packaged privileged transaction staging authority
├── repository/     signed metadata, channels, revocation, transparency
├── runtime/        signed runtime descriptors and compatibility resolver
└── tests/          interruption, rollback, compatibility, conflict, trust fixtures
```

Boundary rules:

- `.app` is the application identity, install, execution, and rollback unit.
- Every non-SOL dependency is private to its bundle.
- Only a declared stable SOL Runtime major may be shared; every app also states
  its minimum contract revision and required feature set.
- Install scripts and root hooks are forbidden; registration is declarative.
- CLI and Software UI never mutate stores directly; `sol-packaged` commits.
- The same privileged service may stage manager, recovery, deployment, and app
  transactions, but it cannot activate them by writing trust state. Stage-0 and
  `sol-boot` independently validate their layers; manager, recovery, and
  deployment trials use distinct records and promotion gates.
- App transactions atomically change a preferred version; launch resolves one
  effective compatible version for the booted system, while app data remains
  independently durable.
- Compatibility resolution walks a `sol-packaged`-recorded chain of verified,
  previously activated hashes and never infers order from a version string.
- Updates prepend to that chain; explicit app rollback truncates newer resolution
  candidates, and reinstall starts a fresh chain.
- Launch selects the first non-revoked retained hash compatible with the booted
  system from the preferred version's fallback chain. OS rollback may select an
  older effective app or mark it explicitly unavailable, but never changes the
  preferred pointer, blocks boot, or rewinds app data.
- Garbage collection protects a compatible app version for every retained
  known-good system deployment when one has previously been installed.
