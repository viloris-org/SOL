# ADR-0019: SOL OS product, image, and boot boundary

- **Status:** Accepted (architecture; implementation pending)
- **Date:** 2026-08-22
- **Target phase:** OS rebaseline / Phase 7
- **Supersedes in part:** ADR-0008 distribution scope

## Context

SOL was originally defined as a desktop platform installed on Arch Linux. That
left boot, recovery, system updates, base compatibility, and machine-wide trust
owned by a different product. It also prevented SOL from giving applications a
single system version, runtime, and security contract.

## Decision

SOL is a complete Linux-kernel operating system, not a desktop layer for an
arbitrary host distribution. It owns image composition, releases, boot,
recovery, system updates, application installation, and security policy.

The installed base is a signed, read-only system image with A/B deployment
slots. Mutable user and machine data lives outside those slots. Each deployment
manifest binds its kernel, initrd, root-image digest, runtime descriptors, and
slot generation; kernel/initrd are not independently replaced global files.

`sol-boot` is a signed SOL UEFI executable and policy component that verifies
boot artifacts, selects a deployment, tracks bounded boot attempts, falls back
to the last known-good deployment, and hands off to a non-graphical recovery
environment. The EFI System Partition retains current and fallback `sol-boot`
copies, and recovery is redundant as well. Updating either uses write-inactive,
verify, one-shot trial, and promote-or-fallback semantics. The only known-good
copy is never overwritten in place.

The intended trust chain is platform Secure Boot keys → retained/trial
`sol-boot` → signed deployment manifest binding kernel, initrd, root-image
digest, runtime descriptors, and generation → authenticated userspace system
identity.

SOL reuses the Linux kernel, drivers, UEFI libraries, UKI conventions, systemd,
Mesa, PipeWire, and other upstream components. It does not create its own
firmware, cryptography, driver model, or init system merely to claim ownership.
Arch packages may bootstrap builds during transition but do not define the
installed OS or its public compatibility contract.

## Consequences

- Boot, system update, failure detection, rollback, and recovery are one state
  machine rather than independent scripts.
- Power loss, firmware-variable failure, or a bad EFI/recovery update cannot
  erase the independently addressable known-good boot and recovery paths.
- The desktop substrate remains useful but is no longer the complete product.
- Reproducible image construction, installer, signing infrastructure, recovery,
  boot-success reporting, and hardware-backed validation become release gates.
- The existing pacman split packages remain transitional developer artifacts;
  they cannot demonstrate the target system lifecycle.

## Required tests

- Power loss at every write/verify/trial/promote step leaves a firmware-visible
  known-good `sol-boot` and recovery copy.
- A bad EFI or recovery trial returns to the retained copy without requiring the
  graphical system.
- Firmware-variable write failure cannot remove or reorder the only known-good
  entry.
- A deployment whose kernel, initrd, root digest, runtime descriptor, or
  generation differs from its signed manifest is rejected.
- Garbage collection cannot remove the last proven boot, recovery, or system
  deployment fallback.

## Non-claims

This ADR does not implement an EFI binary, choose the final system-image
filesystem, select the exact UEFI variable/entry encoding, provision Secure
Boot keys, or claim a working installer. It fixes ownership, redundancy, trial
activation, and safety properties before those prototypes.

## Related

- [OS Platform Definition](../os-platform.md)
- ADR-0020 (native package and runtime contract)
- ADR-0021 (application security and permissions)
