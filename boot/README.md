# SOL boot and system image

This directory is reserved for the Phase 7 bootable-system work defined by
[ADR-0019](../docs/decisions/0019-os-product-and-boot-boundary.md).

Planned ownership:

```text
boot/
├── sol-boot/       redundant signed UEFI entries, verification, slot selection
├── recovery/       redundant non-graphical repair/reinstall environment
├── image/          slot-bound kernel/initrd/root-image and manifest builder
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

There is no bootloader implementation here yet. The first implementation task
is an executable state-machine model and fixture tests for interrupted EFI,
recovery, and deployment updates; failed trial boots; firmware-variable failure;
power loss; fallback; and recovery before firmware integration.
