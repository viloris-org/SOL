# ADR-0022: System-managed accounts and credential vault

- **Status:** Accepted (architecture; implementation pending)
- **Date:** 2026-08-22
- **Target phase:** OS rebaseline / Phase 8
- **Extends:** ADR-0021 atomic explicit permissions

## Context

Applications commonly implement their own account databases, OAuth callback
handling, refresh-token persistence, encryption, scope UI, and revocation.
That duplicates sensitive runtime code, makes account reuse inconsistent, and
leaves the OS unable to explain which app can use which identity.

SOL already owns authenticated application identity and permission brokering.
Accounts and credentials must follow the same system boundary.

## Decision

SOL provides two cooperating system services:

- `sol-accountsd` owns device users, connected service accounts, provider
  metadata, account lifecycle, and app-to-account associations.
- `sol-vaultd` owns encrypted passwords, refresh tokens, passkeys, private
  keys, recovery material, and scoped credential leases.

Applications use a Public/Restricted SolKit Accounts API. They receive opaque
`AccountHandle` values and the minimum profile fields explicitly released by
the user. They never read the account database, vault key, another app's
association, or a durable refresh credential.

Where protocols permit, a system broker performs authorization, token refresh,
signing, or request authentication for the app. If a bearer token must cross
the boundary, it is short-lived, audience-bound, scope-bound, app-bound, and
revocable.

Adding an account, selecting one for an app, expanding provider scopes,
exporting credentials, and recovery are trusted system UI. An app × account ×
provider-scope association is one explicit atomic grant under ADR-0021. Account
presence, app installation, first-party signing, or an earlier app release
creates no association automatically.

`sol-securityd` is the sole transaction coordinator and durable permission
ledger. For an account grant, `sol-accountsd` prepares the association and
`sol-vaultd` prepares any required credential lease under the coordinator's
transaction ID. Prepared records are invisible to enumeration and unusable for
authentication. `sol-securityd` commits the grant, audit event, participant
receipts, and monotonic authorization generation together, then gives
participants a verifiable commit proof. Prepare, commit, abort, and recovery are
idempotent.

`sol-vaultd` accepts a lease operation only when its commit proof matches the
current authorization generation in `sol-securityd`. Revocation raises that
generation before association/credential cleanup. Therefore a participant
crash, delayed cleanup, or replayed old handle cannot restore access after the
coordinator reports revocation.

“Offline access” means the external provider/network may be unavailable; the
local authorization boundary is still required. If `sol-vaultd` cannot validate
the current generation with local `sol-securityd` state, credential use fails
closed rather than trusting a cached generation indefinitely.

Service-owned records are encrypted at rest and unlocked only for an
authenticated user session. Hardware-backed sealing is used when available,
with an explicit recovery-key path. A hardware fallback cannot silently reduce
encryption or authentication strength.

Account removal first commits the higher revocation generation and association
invalidation in `sol-securityd`, then `sol-accountsd` and `sol-vaultd` delete or
tombstone their records idempotently. A crash cannot leave credentials usable
through a previously valid broker handle after the UI reports removal.

## Consequences

- Apps carry less authentication and secure-storage runtime code.
- Users get one system surface for account inventory, scope, app access,
  reauthentication, recovery, and removal.
- Provider adapters and account schemas require versioning and migration.
- Offline access must use explicitly granted, bounded cached material rather
  than copying the system credential into app storage.
- Enterprise/device policy can provision associations only as explicit managed
  grants that remain visible and auditable.

## Required tests

- An app cannot enumerate an account without an explicit association grant.
- A grant exposes only approved profile fields and provider scopes.
- A brokered operation cannot be replayed by another App ID.
- Expired/revoked leases fail offline as well as online.
- Credential use fails closed when the local authorization generation cannot be
  validated, even if cached provider material exists.
- Crash injection across account removal never leaves a usable credential.
- Crash injection before and after participant prepare/coordinator commit never
  exposes an uncommitted association or lease, and recovery converges
  idempotently.
- A lease carrying an older authorization generation fails even before delayed
  participant cleanup completes.
- Vault backup/recovery preserves encryption and does not restore revoked app
  associations implicitly.

## Non-claims

This ADR does not choose individual OAuth providers, cloud sync, password-
manager import formats, TPM APIs, or the final encrypted database.

## Related

- [OS Platform Definition](../os-platform.md)
- ADR-0012 (application identity)
- ADR-0021 (atomic explicit permissions)
