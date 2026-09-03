# ADR-0019: SOL OS product, image, and boot boundary

- **Status:** Accepted (architecture; revised 2026-09-02)
- **Date:** 2026-08-22
- **Target phase:** OS rebaseline / Phase 7
- **Supersedes in part:** ADR-0008 distribution scope

## Context

SOL was originally defined as a desktop platform installed on another Linux distribution. That
left boot, recovery, system updates, base compatibility, and machine-wide trust
owned by a different product. It also prevented SOL from giving applications a
single system version, runtime, and security contract.

## Decision

SOL is a complete Linux-kernel operating system, not a desktop layer for an
arbitrary host distribution. It owns image composition, releases, boot,
recovery, system updates, application installation, and security policy.

The installed base is a signed, read-only system image with A/B deployment
slots. Mutable user and machine data lives outside those slots. A deployment's
signed content identity binds its UKI, root-image/dm-verity identity, runtime
descriptors, security version, and generation. Physical slot A or B is placement
state, not the release's cryptographic identity.

Boot is split by trust and recovery responsibility:

```text
UEFI Secure Boot / platform trust root
    ├─ independently firmware-addressable recovery or signed external recovery
    └─ stable, minimal Stage-0 boot anchor
         ├─ retained/trial SOL boot-policy manager copies
         └─ automatic platform-recovery path
              ↓
       signed deployment A/B selection
              ↓
       UKI + dm-verity system image
```

The Stage-0 anchor verifies and selects a boot-policy manager and can enter
platform recovery; it does not parse system-package policy, render product UI,
or manage deployment health. `sol-boot` is the deployment policy manager: it
verifies boot artifacts, consumes bounded attempts, selects a deployment, and
loads a UKI. Recovery is a sibling path that can repair `sol-boot`, not a child
that is reachable only after `sol-boot` succeeds. A firmware boot entry or
signed external recovery medium remains available when the ESP or Stage-0 path
cannot be trusted.

The deployment selector follows four independent concepts rather than treating
"A/B" as a complete safety mechanism:

- **bootable:** the slot contains a complete authorized deployment;
- **tries remaining:** a trial receives a bounded number of transfers;
- **successful:** an authenticated health authority reached the defined gate;
- **priority:** policy preference among bootable deployments.

Mutable selection state may choose only among already authorized artifacts; it
cannot create trust. Redundant files on one ESP protect against a bounded torn
write, but are not independent storage failure domains. State that influences
promotion is authenticated and replay-resistant in the production design.
CRC32 remains only an accidental-corruption detector.

Functional rollback and security rollback are separate. A failed unpromoted
trial may return to the retained deployment. After promotion, a hardware-backed
security epoch or rollback index prevents return to a revoked or security-old
deployment. The rollback index advances only after the new deployment is
successful, so trial activation never makes the retained fallback unbootable.

A success report binds an unpredictable attempt identity and a measured boot
identity, but authentication alone does not prove semantic health. The health
contract defines at least verified-root, recovery/update-service availability,
and shared-data compatibility checkpoints. Irreversible shared-data migrations
must occur only after the rollback barrier, or use a snapshot/versioning scheme
that keeps the retained deployment usable.

The intended trust chain is platform Secure Boot keys → Stage-0 anchor →
retained/trial `sol-boot` manager → signed deployment identity binding the UKI,
dm-verity root, runtime, generation, security version, and key epoch →
authenticated userspace system identity. Secure Boot disabled or unlocked mode
is explicitly degraded and cannot claim verified boot.

SOL reuses the Linux kernel, drivers, UEFI libraries, UKI conventions, systemd,
Mesa, PipeWire, and other upstream components. It does not create its own
firmware, cryptography, driver model, or init system merely to claim ownership.

## Consequences

- Boot-manager selection, deployment trials, recovery selection, and data
  migration are separate state machines joined by explicit compatibility and
  commit rules rather than one overloaded A/B record.
- Recovery remains reachable without executing the component it may need to
  repair.
- Power loss or a bad boot-manager/deployment trial retains an authorized path
  within the explicitly supported storage-failure boundary.
- The desktop substrate remains useful but is no longer the complete product.
- Reproducible image construction, installer, signing infrastructure, recovery,
  authenticated boot-success reporting, rollback protection, and shared-data
  compatibility become release gates.
- The existing pacman split packages remain transitional developer artifacts;
  they cannot demonstrate the target system lifecycle.

## Required tests

- Power loss at every write/verify/trial/promote step leaves Stage-0 able to
  reach a retained manager or platform recovery.
- A corrupt or returning boot-policy manager cannot prevent entry through the
  independent recovery path.
- A bad manager or deployment trial returns to the retained copy without
  requiring the graphical system.
- Firmware-variable write failure cannot remove the only recovery route.
- A deployment whose kernel, initrd, root digest, runtime descriptor, or
  generation differs from its signed manifest is rejected.
- Replayed selection state, boot-success reports, security policy, or revoked
  deployments cannot lower the accepted security epoch.
- Promotion is impossible until the verified-root, repairability, and shared-
  data compatibility gates for the exact attempt have passed.
- Garbage collection cannot remove the last proven boot, recovery, or system
  deployment fallback.

## Non-claims

This ADR does not implement an EFI binary, choose the final system-image
filesystem, select the exact UEFI variable/entry/TPM encoding, provision Secure
Boot keys, guarantee recovery from physical-device loss, or claim a working
installer. Multiple files on one ESP are not claimed as protection against ESP
or disk failure. It fixes ownership, trust layering, recovery independence,
trial activation, rollback, and shared-data safety properties before those
prototypes.

## Related

- [OS Platform Definition](../os-platform.md)
- [Android A/B system updates](https://source.android.com/docs/core/ota/ab)
- [Android Verified Boot](https://source.android.com/docs/security/features/verifiedboot)
- [Apple silicon boot modes](https://support.apple.com/guide/security/sec10869885b/web)
- [Apple LocalPolicy](https://support.apple.com/guide/security/secc745a0845/web)
- ADR-0020 (native package and runtime contract)
- ADR-0021 (application security and permissions)
