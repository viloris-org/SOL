# SOL boot and system image

This directory is reserved for the Phase 7 bootable-system work defined by
[ADR-0019](../docs/decisions/0019-os-product-and-boot-boundary.md).

Planned ownership:

```text
boot/
├── sol-boot/       redundant signed UEFI entries, verification, slot selection
├── recovery/       redundant non-graphical repair/reinstall environment
├── sol-image/      slot-bound kernel/initrd/root-image manifest builder
└── tests/          boot state-machine and virtual/hardware fixtures
```

Boundary rules:

- Boot state never depends on the graphical Shell.
- Slot attempts, success, fallback, and recovery form one durable state machine.
- Kernel and initrd are bound to a complete system deployment; they are never
  updated as uncoordinated global files.
- `sol-boot` and recovery updates write and verify an inactive copy, trial it
  once, and retain a firmware-visible known-good fallback until promotion.
- User data is outside system slots.
- Verification uses audited upstream cryptography; SOL owns policy and UX.
- No component may mark a slot good before the required userspace health gate.

[`sol-image`](sol-image/README.md) now provides the first executable Phase 7
boundary: byte-reproducible deployment manifests and full artifact verification.
There is no bootloader implementation here yet. The next implementation task is
an executable state-machine model and fixture tests for interrupted EFI,
recovery, and deployment updates; failed trial boots; firmware-variable failure;
power loss; fallback; and recovery before firmware integration.
