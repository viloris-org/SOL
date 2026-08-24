# sol-boot

`sol-boot` is SOL's x86-64 UEFI boot policy adapter. It is now usable for
development images: it builds as a PE32+ EFI application, verifies release-key
signed slot descriptors and the complete manifest/UKI bytes, selects redundant
A/B state, durably consumes trial attempts before transfer, promotes an exact
success report, falls back to a verified known-good deployment, and invokes a
UKI through UEFI `LoadImage`/`StartImage`.

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

## ESP contract

```text
EFI/SOL/
├── sol-boot.efi
├── slots/A/{deployment.bin,manifest.json,system.efi}
├── slots/B/{deployment.bin,manifest.json,system.efi}
├── state/{state-a.bin,state-b.bin,current.bin?,success.bin?,recovery.request?}
└── recovery/{recovery-a.efi,recovery-b.efi}
```

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

Before starting a trial UKI, `sol-boot` writes `state/current.bin` containing
the exact slot/generation/attempt report template. Early userspace copies that
file to `success.bin` only after its health gate. The CLI can also construct the
same canonical bytes for test and recovery tooling:

```bash
cargo run -p sol-image -- success-report \
  --slot B --generation 2 --attempt 1 \
  --output "$ESP/EFI/SOL/state/success.bin"
```

Creating `state/recovery.request` requests recovery once; the bootloader
removes it before trying `recovery-a.efi`, then `recovery-b.efi`.

## Validation and current qualification boundary

Run `scripts/test-ovmf.sh` after building to verify that OVMF executes the
application and reaches its fail-closed recovery path. Host tests cover
signature/artifact corruption, A/B fallback, exact durable read-back, report
promotion, EDID parsing/mode decisions, and bounded rendering.

The current UEFI adapter preserves the usable firmware mode and renders via
GOP BLT. EDID preferred-mode parsing and selection are implemented and tested,
but the EDID Active protocol is not yet wired into the safe UEFI adapter.
Success-report authenticity currently relies on the installer/early-userspace
protection of the ESP transport; TPM-backed report authentication and physical
hardware seamless-handoff qualification remain release blockers, not blockers
for development boot use.
