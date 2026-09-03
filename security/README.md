# SOL application security

This directory contains the Phase 8 security coordinator foundation defined by
[ADR-0021](../docs/decisions/0021-application-security-permissions.md).

Planned ownership:

```text
security/
├── sol-securityd/  SCP identity, policy, signed-token and audit daemon
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

`sol-securityd` listens on `/run/sol/securityd.sock` by default. Its private
state lives under `/var/lib/sol/security`; all input files must be daemon-owned
regular files with no group/world write bits or the daemon fails closed.

The package activator publishes authenticated process mappings in
`identities.tsv`:

```text
version\t1
identity\torg.example.Editor\t1000\t/opt/apps/Editor.app/bin/editor
```

The policy administrator publishes the authorization ledger atomically. Every
change, including revocation, increments `generation`; tokens from older
generations then stop verifying immediately:

```text
version\t1
generation\t42
grant\torg.example.Editor\tclipboard-read\tallow
grant\torg.example.Editor\tscreen-capture-output\tprompt
```

The current coordinator covers the compositor boundary: exact executable + UID
attribution, default-deny policy, HMAC-scoped expiring handles, allow-once replay
protection, generation fencing, and durable audit-before-issuance. Sandbox
construction, publisher/bundle activation into the identity registry, trusted
Shell consent resolution, and account/vault participant transactions remain
separate Phase 8 work.

The current `sol-system` permission stores remain API foundations and test
fixtures, not this production enforcement boundary.
