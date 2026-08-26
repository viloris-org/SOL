# `sol-bundle`

`sol-bundle` implements the application signing design in
[ADR-0029](../../../docs/decisions/ADR-0029-app-signing-publisher-lineage.md).
It provides a reusable Rust library and the Phase 8 signing CLI.

Implemented security properties:

- Canonical, sectioned `manifest.json` covering every regular bundle file.
- Ed25519 and ECDSA P-256 signing and verification.
- RSA-4096/SHA-256 signing and verification for legacy publishers.
- Canonical protobuf `v2.sig` with all-or-nothing multi-signer verification.
- Publisher key rotation and bounded, cycle-resistant lineage verification.
- Strict `version_code` anti-replay checks and primary-lineage grant continuity.
- Optional cached key revocation checks.
- Rejection of symbolic links, undeclared executable content, added unsigned
  files, non-canonical signature metadata, and unexpected lineage entries.

An application manifest must contain the signed identity fields:

```toml
[app]
app_id = "com.example.editor"
version = "2.4.1"
version_code = 241
```

Typical flow:

```bash
cargo run -p sol-bundle -- keygen --out publisher-a.pem
cargo run -p sol-bundle -- sign Example.app --key publisher-a.pem
cargo run -p sol-bundle -- verify Example.app --show-lineage

cargo run -p sol-bundle -- keygen --out publisher-b.pem
cargo run -p sol-bundle -- rotate-key \
  --old-key publisher-a.pem --new-key publisher-b.pem \
  --reason key_expiry --out lineage.bin
cargo run -p sol-bundle -- sign Example.app \
  --key publisher-b.pem --lineage lineage.bin
```

Keys are PKCS#8 PEM files. `keygen` creates them with mode `0600` on Unix and
refuses to overwrite an existing path. Use `--algorithm ecdsa-p256` or
`--algorithm rsa4096` consistently on key generation and signing when selecting
a non-default algorithm.
