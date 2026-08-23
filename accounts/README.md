# SOL system-managed accounts

This directory is reserved for the Phase 8 account and credential work defined
by [ADR-0022](../docs/decisions/0022-system-managed-accounts.md).

Planned ownership:

```text
accounts/
├── service/        sol-accountsd metadata, lifecycle, prepared associations
├── vault/          sol-vaultd encryption, commit-proof-bound credential leases
├── providers/      versioned OAuth/passkey/service adapters
├── broker/         scoped authenticated operations for applications
├── recovery/       explicit recovery-key and reauthentication flows
└── tests/          isolation, revocation, crash, migration, recovery fixtures
```

Boundary rules:

- SOL owns durable account and credential storage; applications do not.
- Apps see only opaque account handles and explicitly released profile fields.
- Every app × account × provider-scope association is an explicit atomic grant.
- `sol-securityd` is the sole transaction coordinator and durable permission
  ledger; account/vault records prepared under a transaction ID are unusable
  until its commit proof is verified.
- Revocation advances an authorization generation before cleanup, and every
  account/vault operation rejects stale generations.
- Durable refresh credentials stay in the vault whenever the protocol allows a
  brokered or short-lived alternative.
- Account removal revokes leases and associations before credential deletion.
- Hardware-backed storage may strengthen the vault but cannot make recovery
  ambiguous or silently fall back to plaintext.

There is no account daemon or encrypted vault implementation here yet.
