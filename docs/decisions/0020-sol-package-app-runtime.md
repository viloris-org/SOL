# ADR-0020: SOL package manager, `.app`, and shared runtime contract

- **Status:** Accepted (architecture; implementation pending)
- **Date:** 2026-08-22
- **Target phase:** OS rebaseline / Phases 8–9
- **Supersedes:** ADR-0018 package backend decision
- **Supersedes in part:** ADR-0008 distribution scope

## Context

Using pacman/AUR as the native application backend exposes host dependency
resolution, install scripts, global library mutation, and a trust model that is
not aligned with an identity-scoped application sandbox. Fully bundling every
runtime avoids dependency conflicts but duplicates the common platform and
makes coherent framework evolution difficult.

## Decision

SOL owns `sol-pkg`, its user/admin CLI, and `sol-packaged`, the sole privileged
staging service for boot-manager, recovery, system-deployment, and application
transactions. A Software UI is an unprivileged client of the same service.
Staging authority is not boot trust: Stage-0, `sol-boot`, and the application
runtime independently validate and activate artifacts at their own layers.

The native application unit is a deterministic, signed, relocatable, read-only
`.app` bundle installed in a content-addressed store. It contains a canonical
manifest, signatures, executables, private libraries, resources, extensions,
licenses, SBOM, and provenance. It has no package-time root script.

Every app vendors all non-SOL userspace dependencies. Those dependencies are
private to the bundle and never participate in global dependency resolution.
The only shared application platform is a declared SOL Runtime major, such as
`sol-runtime-1`. The signed manifest also declares a minimum monotonically
increasing contract revision and any required stable feature names. Runtime
majors can coexist. Compatible changes preserve a major slot, advance the
revision, and do not remove older features; breaking changes require a new slot.

Installed apps do not rely on Rust ABI or arbitrary libraries from the system
image. Stable in-process boundaries use a C-compatible ABI where required;
system capabilities use versioned IPC. SolKit provides source-level language
bindings over those contracts.

System-image and app operations both use `resolve → fetch → verify → stage →
validate → commit`. Manager, recovery, and deployment activation then use
their separate ADR-0019/0026 trial records and promotion gates; a package
transaction cannot create boot trust by writing selection state. System
deployments activate through physical A/B slots. An app
transaction atomically switches its preferred bundle hash; launch derives an
effective bundle hash compatible with the current deployment's authenticated
runtime descriptor. Neither rollback rewinds user data.

On launch, `sol-packaged` walks an ordered fallback chain of previously verified,
successfully activated hashes in the same durable security identity and channel,
starting at the preferred hash. It selects the first non-revoked version
satisfying the current runtime major, minimum contract revision, and required
features; display version strings never determine ordering. An OS rollback
repeats this resolution without rewriting the preferred pointer and may
reactivate an older retained app version. Returning to a compatible newer system
restores the preferred version. If none is compatible, the app becomes explicitly
unavailable for that system version rather than blocking boot. Garbage collection
protects a compatible version for every retained known-good system deployment
whenever one has previously been installed.

An app update prepends its hash to the existing chain. Explicit app rollback
selects an existing hash and truncates newer descendants from resolution, so a
later OS transition cannot silently undo the user's rollback. Reinstall starts a
new chain; retained content alone is not activation state.

## Consequences

- Conflicting third-party library versions can coexist without dependency
  solving or filesystem collisions.
- Apps using SOL Runtime avoid bundling common UI, lifecycle, accessibility,
  localization, and portal clients.
- Repository and publisher signatures cover identity, hashes, capability
  declarations, runtime requirement, and provenance.
- Developer tooling must build, lint, inspect, sign, verify, diff, and test
  `.app` bundles reproducibly.
- Package tooling and system-update validation must compute the compatibility
  matrix for current and known-good fallback deployments; “same runtime major”
  alone is not evidence of compatibility.
- pacman/AUR becomes a build/bootstrap input, not the installed-system package
  authority. Flatpak can only be a compatibility subsystem.

## Required tests

- A runtime major match with an insufficient contract revision or missing
  feature is rejected.
- Launch selects the first non-revoked compatible hash from the recorded fallback
  chain deterministically and never orders by a display version string.
- OS fallback changes only the effective version; it does not rewrite the
  preferred version selected by an app transaction.
- Explicit app rollback truncates newer resolution candidates and they do not
  reappear without another user-approved app update.
- OS rollback selects a retained compatible app version without changing app
  data; if none exists, only that app becomes explicitly unavailable.
- System-update validation reports compatibility for both candidate and
  known-good fallback deployments.
- Garbage collection preserves a compatible app version for every retained
  known-good deployment when one was previously installed.
- Side-by-side runtime majors and incompatible private libraries do not affect
  another app's resolution.

## Rejected alternatives

1. **Keep pacman/libalpm as the hidden native backend.** It preserves global
   dependency and install-hook semantics that conflict with the `.app` and
   sandbox identity unit.
2. **Bundle the complete SOL runtime into every app.** This isolates versions
   but wastes space and prevents SOL from providing a compact stable platform.
3. **Allow linking to arbitrary system libraries.** This recreates dependency
   hell outside the package metadata and makes rollback unsafe.
4. **Use a mutable shared library pool deduplicated by package name.** This
   allows one app's update to change another app's executable environment.

## Non-claims

The `.app` container encoding, compression, delta format, repository protocol,
stable ABI generator, and storage reclamation algorithm still require
prototypes. The garbage collector's compatibility-retention invariant is fixed
even though its eviction policy is not.

## Related

- [OS Platform Definition](../os-platform.md)
- ADR-0012 (application identity)
- ADR-0017 (SolKit stability tiers)
- ADR-0019 (OS and boot boundary)
- ADR-0021 (security and permissions)
- ADR-0024 (non-native toolkit compatibility)
