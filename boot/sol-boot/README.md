# sol-boot

## ⚠️ DEVELOPMENT STATUS - NOT PRODUCTION READY

**Critical security features are NOT yet implemented. See Security Status below.**

`sol-boot` is SOL's x86-64 UEFI deployment-policy manager and current
development firmware adapter. It builds as a PE32+ EFI application, verifies
release-key-signed descriptors and manifest/UKI bytes, selects A/B deployment
state, consumes a trial attempt before transfer, and invokes a UKI through UEFI
`LoadImage`/`StartImage`.

It is not the target Stage-0 anchor and does not by itself prove independent
recovery, authenticated health, anti-replay, manager self-update fallback, or
security rollback protection. Those boundaries are defined by ADR-0019 and
ADR-0026.

## Security Status

### ✅ Implemented (Development Trust Model)

- Ed25519 signature verification of deployment descriptors
- SHA-256 artifact binding (manifest + UKI)
- CRC32 torn-write detection
- Deterministic A/B fallback policy
- Bounded trial attempts
- Best-effort boot logging

### 🔴 NOT Implemented (Required for Production)

1. **Authenticated State Storage** - `boot/sol-boot-core/src/auth.rs`
   - Current: CRC32 integrity checking only
   - Required: HMAC-SHA256 authentication + TPM NV replay protection
   - **Risk**: State tampering and replay attacks possible

2. **Security Rollback Protection** - `boot/sol-boot-core/src/rollback.rs`
   - Current: Stub implementation
   - Required: TPM 2.0 monotonic security version index
   - **Risk**: Revoked deployments can still boot

3. **Stage-0 Independent Recovery**
   - Current: Recovery invoked through sol-boot itself
   - Required: Firmware-addressable recovery independent of manager

**DO NOT deploy to production until Phase 7 security components are complete.**

See [ADR-0026](../../docs/decisions/0026-sol-boot-uki-and-graphics-handoff.md) 
for detailed security requirements and threat model.

## Build

The release public key is compiled into the EFI binary. Keep the 32-byte
Ed25519 signing seed offline and pass only its public key to the build:

```bash
openssl rand 32 > release.key
export SOL_BOOT_PUBLIC_KEY_HEX=$(cargo run -q -p sol-image -- \
  release-public-key --signing-key release.key)
boot/sol-boot/scripts/build-uefi.sh
```

The result is `target/x86_64-unknown-uefi/release/sol-boot.efi`. Signing that
PE image for platform Secure Boot is a separate release step; `sol-boot` never
embeds the private release key.

## Development ESP contract

```text
EFI/SOL/
├── sol-boot.efi
├── slots/A/{deployment.bin,manifest.json,system.efi}
├── slots/B/{deployment.bin,manifest.json,system.efi}
├── state/{state-a.bin,state-b.bin,current.bin?,success.bin?,recovery.request?}
└── recovery/{recovery-a.efi,recovery-b.efi}
```

This layout is a development format on one ESP. Its duplicate state records
provide bounded torn-write tolerance, not an independent ESP or disk failure
domain. The target layout adds a stable Stage-0, retained/trial manager copies,
and a firmware-addressable recovery path.

`deployment.bin` is a fixed 168-byte record. Its Ed25519 signature covers the
slot, generation, architecture, exact manifest digest/length, and exact UKI
digest/length. The manifest must be canonical format 2. When platform Secure
Boot is enabled, firmware independently verifies the PE signature when
`LoadImage` accepts the UKI or recovery image.

Provision the first known-good slot with:

```bash
cargo run -p sol-image -- boot-descriptor \
  --slot A --generation 1 \
  --manifest "$ESP/EFI/SOL/slots/A/manifest.json" \
  --uki "$ESP/EFI/SOL/slots/A/system.efi" \
  --signing-key release.key \
  --output "$ESP/EFI/SOL/slots/A/deployment.bin"

cargo run -p sol-image -- init-boot-state \
  --slot A --generation 1 \
  --state-a "$ESP/EFI/SOL/state/state-a.bin" \
  --state-b "$ESP/EFI/SOL/state/state-b.bin"
```

After fully staging and verifying the inactive slot, register its bounded
trial. Either state copy remains independently usable if power fails between
the two writes:

```bash
cargo run -p sol-image -- stage-boot-trial \
  --slot B --generation 2 --attempts 3 \
  --state-a "$ESP/EFI/SOL/state/state-a.bin" \
  --state-b "$ESP/EFI/SOL/state/state-b.bin"
```

Before starting a trial UKI, the development manager writes
`state/current.bin` containing
the exact slot/generation/attempt report template. Early userspace copies that
file to `success.bin` only after its health gate. The CLI can also construct the
same canonical bytes for test and recovery tooling:

```bash
cargo run -p sol-image -- success-report \
  --slot B --generation 2 --attempt 1 \
  --output "$ESP/EFI/SOL/state/success.bin"
```

Creating `state/recovery.request` requests recovery through the current manager.
This is useful development scaffolding but is not the target independent
platform-recovery entry: a broken manager cannot consume this request. The
current remove-before-transfer behavior also does not meet the target rule that
recovery acknowledge the request only after it has started.

## Validation and current boundary

Run `scripts/test-ovmf.sh` to build an isolated test-key image and verify both
the fail-closed recovery path and a complete signed A/B trial. The harness
boots the trial payload through UEFI `LoadImage`/`StartImage`, checks the exact
durable attempt report, submits it as the health result, and verifies that the
next boot promotes the deployment to known-good. Host tests cover signature/
artifact corruption, authorized A/B fallback, exact durable read-back, and
report promotion.

The current adapter is display-absent. The target policy permits, but does not
require, one static centered mark in the current GOP mode. It never reads EDID,
chooses a native/preferred resolution, or calls `SetMode()`. Missing or broken
graphics must be ignored. Native DRM, scaling, multi-display, and all
interactive boot/recovery UI belong to the selected UKI.

If firmware rejects or returns from a selected trial UKI, `sol-boot` re-verifies
and starts only the exact retained known-good deployment authorized by durable
state. It then attempts `recovery-a.efi` and `recovery-b.efi`. If no image can
start, it returns `LOAD_ERROR` to firmware without producing output. The OVMF
harness uses distinct child payload markers—not bootloader text—to prove trial,
promotion, runtime A/B fallback, recovery transfer, and silent failure.

The current success flow copies a known template from `current.bin` to
`success.bin`. Exact slot/generation/attempt binding rejects accidental stale
reports, but the ESP transport is neither a production authenticator nor an
anti-replay boundary. The target protocol adds an unpredictable attempt,
measured identity, verified-root/repairability/data-compatibility checkpoints,
and a promotion-gated rollback index.

Stage-0, independent platform/external recovery, manager trials, authenticated
state/reports, content identity independent of slot placement, indivisible UKI
composition, rollback protection, and shared-data migration barriers remain
release work. SOL maintains no certified boot-graphics hardware matrix and does
not promise native-resolution or seamless boot.
