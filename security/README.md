# SOL application security

This directory is reserved for the Phase 8 enforcement work defined by
[ADR-0021](../docs/decisions/0021-application-security-permissions.md).

Planned ownership:

```text
security/
├── identity/       publisher, AppId, bundle hash, process attribution
├── policy/         authoritative grant ledger, coordinator, revocation generations
├── sandbox/        namespaces, cgroups, seccomp, Landlock/LSM projection
├── broker/         scoped handles and portal/service mediation
├── audit/          bounded private security decision records
└── tests/          denial, escape, spoofing, revocation, and scope fixtures
```

Boundary rules:

- The signed manifest limits what may be requested; it grants nothing.
- Every protected capability requires an explicit user or managed-policy grant.
- One grant is one user × App ID/publisher lineage × capability × resource ×
  duration; bundle hash/process generation bind live handles, not durable identity.
- Grant, audit, and handle/lease issuance commit atomically or not at all.
- Unrelated capabilities never hide behind one “Allow all” decision.
- The sandbox is constructed before untrusted code executes.
- Trusted consent UI resolves typed requests but cannot weaken kernel/broker
  enforcement.
- Restricted SDK calls are requests, not ambient authority.
- Same-lineage update/rollback retains durable grants but revokes live handles;
  publisher discontinuity inherits nothing.
- Uninstall revokes leases and grants; reinstall requires new consent even when
  app data is retained.
- `sol-securityd` is the sole coordinator for account/vault participants;
  prepared state is unusable without its commit proof and current generation.
- First-party services use explicit system identities and remain auditable.

The current `sol-system` permission stores are API foundations and test
fixtures, not the production enforcement boundary. Their separate permission
and audit stores do not yet satisfy the unified atomic transaction requirement.
