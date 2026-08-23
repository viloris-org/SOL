# ADR-0021: Default-deny application security and permissions

- **Status:** Accepted (policy; enforcement implementation pending)
- **Date:** 2026-08-22
- **Target phase:** OS rebaseline / Phase 8
- **Extends:** ADR-0013 typed System Action and Permission Layer

## Context

A typed permission API alone does not contain a hostile or compromised process.
The OS needs one authenticated identity from package verification through
process launch, kernel enforcement, user consent, resource brokering,
revocation, and audit.

## Decision

Every third-party `.app` runs under its signature-authenticated `AppId` in a
default-deny sandbox constructed before its entry point executes. The signed
manifest declares which capabilities the app may request. Declaration is not a
grant; every protected capability requires an explicit user or managed
system-policy grant before use.

SOL applies minimum authority structurally. A durable security identity is an
authenticated App ID plus a verified publisher lineage. A release identity adds
the exact signed bundle hash and process generation. One permission atom is
keyed by user, durable security identity, capability, resource scope, and
duration. The broker must reject broad authority when a narrower portal, object
handle, one-shot authorization, or short-lived lease can complete the operation.

Grant persistence, the required audit record, and issuance of the capability
handle/lease are one transaction. The handle is usable only after the commit.
If validation, persistence, audit, or issuance fails, no part of the grant is
effective. Allow-once consumption is also durable and atomic, so it cannot be
replayed after a crash.

`sol-securityd` owns identity attribution, the authoritative grant ledger,
permission-transaction coordination, revocation, sandbox policy, and security
audit. Enforcement combines namespaces, cgroups, seccomp,
Landlock and/or another selected LSM, filesystem ownership, Wayland mediation,
and per-app storage. Protected resources are delivered through typed portals
and capability brokers rather than ambient access.

Consent occurs at point of use and identifies app, publisher, resource,
purpose, scope, and duration. Unrelated capabilities are never combined behind
one “Allow all” decision. A workflow may explain multiple needs on one trusted
surface, but each atom has its own control, commit, and revocation. Users can
inspect and revoke durable grants. Decisions are recorded in bounded private
audit storage.

Restricted SolKit APIs remain callable contracts, not authority. They send
typed, caller-attributed requests to the security boundary and cannot expose an
arbitrary privileged command path. Shipping a private runtime or bypassing the
SDK does not grant more access.

System components use separately signed system identities and explicitly
provisioned service policy. First-party status, installation, account login,
SDK choice, or a previous app version never creates an implicit grant.

## Update, rollback, and uninstall semantics

A normal app update or rollback preserves durable grants only when App ID is
unchanged and publisher continuity is verified. Publisher-key rotation requires
a signature-covered continuity proof from the prior lineage or an explicitly
trusted publisher-recovery path. A discontinuous publisher creates a new
security identity and receives no grant inheritance.

Bundle activation revokes all live handles and leases bound to the prior release
or process generation. A replacement process requests fresh handles; the broker
revalidates the declaration, durable grant, scope, duration, and current policy.
Newly declared capabilities are merely requestable and remain ungranted.

Uninstall first fences and revokes outstanding leases, then marks every durable
grant for that security identity revoked. Reinstalling the same App ID requires
new consent even if app data was retained. Data retention and authority
retention are independent choices.

## Coordinated participant transactions

For grants involving another privileged service, `sol-securityd` is the sole
coordinator. A participant such as `sol-accountsd` or `sol-vaultd` prepares
state under a transaction ID, but prepared associations and leases are unusable.
`sol-securityd` atomically records the grant, audit event, participant receipts,
and authorization generation, then produces a verifiable commit proof.
Participants activate idempotently only after validating that proof.

On recovery, uncommitted preparations abort and committed transactions replay.
Revocation commits a higher authorization generation before cleanup; every
broker rejects older generations, so delayed cleanup or a participant crash
cannot restore authority. This is the externally atomic boundary even though
physical records live in multiple services.

## Consequences

- App ID, publisher identity, bundle hash, process credentials, data paths,
  permission grants, and audit records must stay correlated.
- Portal grants must be unforgeable, scoped, time-bounded where appropriate,
  and invalid after revocation.
- Production storage must expose one transactional grant/audit/lease boundary;
  independent files or best-effort rollback are insufficient.
- Permission UI is trusted Shell surface and must not be spoofable by an app.
- Kernel/broker integration tests, not screenshots of prompts, are the release
  evidence for denial and isolation.

## Required security tests

- undeclared and ungranted access fails without a prompt;
- declaration, installation, first-party signing, and account presence grant
  nothing by themselves;
- failure at any step of grant + audit + lease issuance leaves no authority;
- allow-once cannot be replayed after consumption or app restart;
- revocation invalidates future access and bounded outstanding handles;
- a selected document grant does not reveal its parent directory;
- unverified bundle replacement or publisher discontinuity cannot inherit
  grants;
- same-lineage update/rollback preserves durable grants but invalidates old
  handles; new declarations remain ungranted;
- uninstall revokes leases/grants and reinstall requires new consent;
- direct calls to private services do not bypass broker attribution;
- denial remains effective when the app ships its own toolkit or runtime;
- crash injection before/after participant preparation and coordinator commit
  never exposes an uncommitted association or credential lease.

## Non-claims

This ADR does not select the final LSM combination, define every capability, or
claim that current `sol-system` in-memory/file stores constitute production
enforcement. In particular, the current separate permission and audit stores
do not satisfy the atomic production transaction defined here.

## Related

- [OS Platform Definition](../os-platform.md)
- ADR-0012 (application identity)
- ADR-0013 (typed action and consent API foundation)
- ADR-0019 (OS trust chain)
- ADR-0020 (`.app` package and runtime)
- ADR-0022 (system-managed accounts and credential vault)
- ADR-0025 (trusted global chrome and Live Capsule)
