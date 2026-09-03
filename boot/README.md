# SOL boot and system image

This directory contains the Phase 7 foundations defined by
[ADR-0019](../docs/decisions/0019-os-product-and-boot-boundary.md) and
[ADR-0026](../docs/decisions/0026-sol-boot-uki-and-graphics-handoff.md).

Target ownership:

```text
boot/
├── stage-0/        stable firmware entry and manager/recovery selection
├── sol-boot/       deployment-policy manager and UEFI adapter
├── sol-boot-core/  firmware-independent deployment selector
├── recovery/       platform, paired, and external recovery artifacts
├── sol-image/      deployment identity and image construction
└── tests/          host fault injection and OVMF fixtures
```

`stage-0/` and `recovery/` describe target components; their absence today is
an implementation gap, not a reason to collapse their authority into
`sol-boot`.

## Boundary rules

- UEFI Secure Boot roots a stable minimal Stage-0. Stage-0 selects retained or
  trial `sol-boot` managers and can enter platform recovery.
- Platform recovery is independently firmware-addressable and does not require
  the manager it may repair. Signed external recovery covers an unusable ESP or
  storage device.
- Automatic exhaustion, a durable software request, and a firmware/physical
  manual action can enter recovery; a request is acknowledged only after the
  recovery environment starts.
- Manager, recovery, and deployment trials use separate state and promotion
  gates. They are not interchangeable A/B records.
- A production slot exposes explicit priority, bootable, tries-remaining, and
  successful state. An attempt is consumed before transfer.
- Signed deployment content identity is independent of physical slot A/B. A
  complete production UKI contains kernel, initrd, immutable command line, and
  release metadata and binds the dm-verity root.
- Mutable state may select only already authorized artifacts. CRC and redundant
  same-ESP files detect/recover bounded torn writes; they do not authenticate
  state or protect against ESP/disk failure.
- Functional fallback precedes promotion. A replay-resistant security rollback
  index advances only after promotion and then rejects revoked/security-old
  deployments.
- A success report identifies the exact measured attempt but does not prove the
  system is bug-free. Promotion requires verified-root, repairability, and
  shared-data compatibility checkpoints.
- Irreversible shared-data migration occurs after the rollback barrier or uses
  a snapshot/versioning contract the retained deployment can consume.
- User data remains outside system slots and is never implicitly rewound.
- Verification uses audited upstream cryptography; SOL owns policy and UX.

## Graphics boundary

Stage-0 is display-absent. `sol-boot` may draw one static centered mark in the
current GOP mode. It never reads EDID, chooses a native/preferred resolution,
or calls `SetMode()`. Missing/broken graphics are ignored and cannot affect
verification, retry, fallback, or recovery.

The selected UKI owns native DRM, scaling, multi-display policy, unlock and
recovery UI, and compositor startup. A blank or mode transition during native-
driver takeover is allowed. SOL does not maintain a certified boot-graphics
hardware matrix or promise native-resolution/seamless boot.

## Current implementation boundary

[`sol-image`](sol-image/README.md) implements versioned development manifests.
[`sol-boot-core`](sol-boot-core/README.md) implements deterministic A/B trial
selection and torn-write fixtures. [`sol-boot`](sol-boot/README.md) implements
the current x86-64 UEFI deployment manager and development success-report path.

These foundations do not yet implement Stage-0, independent recovery,
authenticated/replay-resistant state and reports, the rollback index,
content-identity/placement separation, shared-data rollback barriers, or the
final indivisible UKI composition. OVMF evidence proves only the paths it
executes; it is not evidence of firmware-independent recovery or a certified
hardware class.
