# SOL boot and system image

This directory is reserved for the Phase 7 bootable-system work defined by
[ADR-0019](../docs/decisions/0019-os-product-and-boot-boundary.md) and the
[boot execution and graphics handoff ADR](../docs/decisions/0026-sol-boot-uki-and-graphics-handoff.md).

Planned ownership:

```text
boot/
├── sol-boot/       redundant signed UEFI entries, verification, slot selection
├── sol-boot-core/  firmware-independent boot state machine and policy
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
- `sol-boot` reuses signed UKIs and the Linux EFI path; it does not implement a
  Linux loader, firmware driver, filesystem driver, or cryptographic primitive.
- The first SOL frame uses the EDID-preferred resolution only when GOP exposes
  that exact mode. Once selected, Linux and the compositor preserve it whenever
  the hardware permits a mode-preserving handoff.
- Graphics failure degrades presentation only; it cannot change verification,
  retry, fallback, or recovery policy.

[`sol-image`](sol-image/README.md) provides versioned, byte-reproducible
deployment manifests and full format-2 UKI artifact verification.
[`sol-boot-core`](sol-boot-core/README.md) provides the first executable boot
policy: firmware-independent A/B trials, consume-before-transfer attempt
ordering, exact success-report promotion, known-good fallback, and recovery
selection.

[`sol-boot`](sol-boot/README.md) now provides the x86-64 UEFI adapter and
release-key signed deployment envelope. It reads the conventional ESP layout,
verifies complete manifests and UKIs, commits redundant state with exact
read-back, renders a bounded static GOP frame, and starts verified UKIs or
independent recovery images. It has been cross-built as a PE32+ EFI application
and executed under OVMF. TPM-backed success-report authentication, EDID Active
protocol wiring, recovery/boot-authority self-update trials, and hardware
seamless-handoff qualification remain release work.
