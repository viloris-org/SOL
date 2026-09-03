# ADR-0026: SOL boot execution, recovery topology, and best-effort graphics

- **Status:** Accepted (architecture; revised 2026-09-02)
- **Date:** 2026-08-24
- **Target phase:** Phase 7
- **Extends:** ADR-0019

## Context

ADR-0019 makes SOL responsible for verified deployment selection, bounded
trials, rollback, and recovery. The first implementation combined too many
meanings under `sol-boot`: firmware entry, deployment policy, self-update
fallback, recovery dispatcher, and presentation owner. It also treated two
files on one ESP as though they were independent failure domains and left a
candidate userspace able to promote itself through an unauthenticated success
file.

Android provides a useful separation between bootable-slot state, bounded
tries, userspace success, verified artifacts, and rollback indexes. Apple
Silicon provides a useful recovery topology: normal boot, paired recovery, and
fallback recovery are distinct paths rooted below the operating system they
repair. SOL adopts those separations without assuming vendor-controlled
firmware, a Secure Enclave, APFS, or an OEM-maintained hardware matrix.

UEFI Graphics Output Protocol (GOP) exposes only firmware-supported modes.
Calling `SetMode()` may clear the screen, while native Linux drivers may reset
the GPU or retrain a display link. SOL does not maintain a certified boot-
graphics hardware matrix, so native-resolution or flicker-free boot cannot be a
release promise. A restrained static frame is sufficient when the current GOP
mode is usable; graphics remain outside boot correctness.

## Decision

### 1. Separate the platform anchor, boot policy, and recovery

The target x86-64 UEFI topology is:

```text
UEFI Secure Boot
    ├─ firmware-addressable or signed external platform recovery
    └─ minimal Stage-0 anchor
         ├─ SOL boot-policy manager A/B
         └─ automatic platform recovery
                 ↓
          deployment selector
                 ↓
          signed UKI A/B + dm-verity root
```

Stage-0 is deliberately stable and small. It verifies and selects retained or
trial boot-policy managers and enters platform recovery when neither can run.
It does not understand application packages, system runtime descriptors,
deployment health, EDID, or product UI. The concrete Stage-0 component and
UEFI entry encoding remain a Phase 7 decision; a SOL-authored first-stage
loader is not a product requirement when an audited upstream component can
satisfy the contract.

`sol-boot` is the boot-policy manager, not its own ultimate recovery anchor.
Platform recovery is independently reachable and can repair Stage-0, manager
copies, deployment state, and the ESP. A deployment-paired recovery environment
may additionally repair that deployment's root and compatible mutable data,
but receives no implicit authority to lower another deployment's security
policy. A signed external recovery medium is the final path for ESP or storage
failure.

Automatic exhaustion, a durable software request, and a firmware/physical
manual action can all request recovery. A software request is not the only
manual path and is acknowledged only after recovery has started; deleting it
before transfer would lose user intent on a power failure.

Boot-manager copies, recovery images, and deployment slots use different state
types and independent trials. They are not three meanings of the same A/B
record and are never promoted as one transaction. Compatibility is advanced in
layers: a new manager must first prove it can boot the retained deployment and
understand both old and new metadata; recovery is proven separately; only then
may a deployment requiring the new format become a trial.

### 2. Keep deployment policy deterministic and mutable state non-authorizing

`sol-boot-core` remains a deterministic, firmware-independent deployment
selector. It consumes already validated observations and emits actions. The
adapter performs verification and durable I/O. Before transferring to a trial,
the adapter consumes and reads back one attempt.

The production slot model contains Android-like, explicitly separate fields:

```text
deployment_id  priority  tries_remaining  bootable  successful
```

The signed deployment identity is content-addressed and independent of whether
it is installed in physical slot A or B. Placement metadata binds that identity
to the root device used for a particular boot. Mutable state may choose only an
authorized deployment; it cannot authorize new bytes.

Redundant state records on one ESP protect only against the tested torn-write
model. They do not constitute independent protection from FAT metadata, ESP,
controller, or disk failure. Production selection and promotion state must be
authenticated and replay-resistant. CRC32 remains useful only for accidental
corruption detection.

### 3. Reuse UKI and dm-verity as one deployment identity

Each production deployment provides a Unified Kernel Image containing the Linux
EFI stub, kernel, initrd, immutable command line, and release metadata. An
external initrd is not part of the production verified-boot contract. A live or
development image that still loads one is explicitly non-production until the
external artifact is independently verified before use.

The top-level signed deployment manifest binds at least:

- deployment content ID and format version;
- complete UKI digest and byte length;
- logical kernel and initrd component identities;
- immutable root-image digest/length and dm-verity root hash;
- root format and partition-role identity without a physical A/B suffix;
- architecture, generation, system version, and runtime descriptors;
- signing-key epoch, security version, and compatibility constraints.

The manifest signature authorizes the complete SOL deployment. The UKI PE
signature lets firmware Secure Boot authorize executable bytes. Both are
required in locked production mode. Secure Boot disabled/unlocked mode is
reported as degraded and is not called verified boot.

Normal boot hashes the bounded UKI and manifest, not the entire root image.
The kernel verifies root blocks against the signed dm-verity identity as they
are read.

### 4. Separate functional fallback from security rollback

A failed, unpromoted trial can return to the retained successful deployment.
Only after the new deployment passes its promotion gates may the trusted
rollback index advance. Functional rollback remains possible among deployments
at or above the current security floor. Once the floor advances, revoked or
security-old deployments below it are rejected even if their signatures remain
cryptographically valid.

During a trial, both the retained and candidate security epochs may be accepted
so a failed candidate cannot strand the machine. The final authenticated,
replay-resistant storage mechanism may use TPM NV or a platform facility with
equivalent properties; ordinary ESP files are insufficient.

### 5. Define success as staged operational checkpoints

A success report binds the exact deployment, generation, unpredictable attempt
identity, and measured boot identity. Copying a bootloader-created template to
another ESP filename is development scaffolding, not production authentication.

At minimum the production health protocol distinguishes:

1. the UKI started;
2. the signed dm-verity root mounted successfully;
3. essential recovery and update services can repair the machine;
4. shared mutable data is compatible with the retained deployment or protected
   by a usable snapshot/versioning boundary;
5. promotion and rollback-index advancement are permitted.

Authentication proves which measured system emitted an observation; it does
not prove that the system is bug-free. Shell readiness, animation, or display
continuity never blocks the minimum repairability gate. Irreversible firmware,
database, account, or user-data migrations occur only after the defined
rollback barrier, or remain readable by the retained deployment.

### 6. Make boot graphics optional and current-mode-only

Stage-0 performs no graphics work. `sol-boot` may render one bounded static SOL
frame through the currently active GOP mode, but graphics are best-effort:

- never read EDID or infer a native/preferred resolution;
- never enumerate modes for selection or call `SetMode()`;
- preserve the current width, height, stride, and pixel format;
- draw only a solid background and aspect-correct centered mark;
- perform no bootloader animation, interactive menu, or routine text output;
- ignore missing GOP, unsupported pixel formats, and rendering failure;
- never let graphics change verification, retry, fallback, or recovery.

The selected system or recovery UKI owns native DRM initialization, scaling,
multi-display policy, unlock/recovery UI, and all interactive presentation. A
native driver may cause a visible blank or mode change. The compositor may
reuse an active mode when convenient, but mode preservation is an optimization,
not a boot invariant or release gate.

```text
firmware-selected display state
        ↓ optional static draw in the unchanged GOP mode
sol-boot best-effort frame, or untouched firmware frame
        ↓ UKI starts
Linux native DRM may perform a visible mode transition
        ↓
system or recovery UI
```

## Consequences

- Recovery no longer depends on successful execution of the manager it may
  need to repair.
- `sol-boot-core` remains useful but is scoped as deployment-selection policy,
  not proof of the whole boot/recovery authority.
- A/B protects update availability; signatures protect artifact authenticity;
  a hardware-backed index protects against security rollback.
- A same-ESP redundant copy is described honestly as torn-write tolerance, not
  an independent storage failure domain.
- Production UKIs are indivisible kernel/initrd/command-line artifacts.
- SOL makes no native-resolution, no-black-frame, seamless-handoff, certified-
  GPU, panel, dock, or multi-display boot promise.
- Boot presentation stays visually restrained without importing a GPU driver,
  EDID policy, desktop toolkit, or hardware qualification program into UEFI.

## Required tests

### Deterministic host tests

- Slot priority, bootable, successful, and tries-remaining transitions are
  independent and consume an attempt before transfer.
- Mutable state cannot authorize an unknown deployment.
- Torn writes recover within the stated single-ESP failure model.
- Replayed state, reports, or policy cannot lower the accepted security epoch.
- Rollback index advancement occurs only after the exact trial is promoted.
- Every power-loss point preserves either the old data format or a snapshot
  that the retained deployment can use.

### OVMF integration tests

- Stage-0 can start retained/trial managers and reach platform recovery when
  both managers fail.
- Recovery is directly firmware-addressable without a working `sol-boot`.
- A rejected or returning trial falls back to the retained deployment.
- A stale, copied, or unauthenticated success report cannot promote a trial.
- Missing or broken GOP does not alter the selected boot action.
- If static drawing is enabled, `SetMode()` and EDID protocols are never used.

Real-machine smoke tests may report firmware-specific defects, but SOL does not
maintain a certified boot-graphics hardware matrix and they do not establish a
seamless-boot class.

## Non-claims

This ADR does not select the final Stage-0 implementation, UEFI entry layout,
TPM protocol, system-image filesystem, delta format, or shared-data snapshot
backend. It does not guarantee recovery from physical storage failure without
external media, firmware quality on arbitrary PCs, a visible boot UI on every
machine, native bootloader resolution, or a flicker-free DRM takeover.

## Related

- [SOL OS Platform Definition](../os-platform.md)
- [SOL boot and system image](../../boot/README.md)
- [Android A/B system updates](https://source.android.com/docs/core/ota/ab)
- [Android Verified Boot](https://source.android.com/docs/security/features/verifiedboot)
- [Apple silicon boot modes](https://support.apple.com/guide/security/sec10869885b/web)
- [Apple LocalPolicy](https://support.apple.com/guide/security/secc745a0845/web)
- [UEFI Graphics Output Protocol](https://uefi.org/specs/UEFI/2.11/12_Protocols_Console_Support.html#graphics-output-protocol)
- [UAPI Unified Kernel Image specification](https://uapi-group.org/specifications/specs/unified_kernel_image/)
- ADR-0019 (OS image and boot boundary)
