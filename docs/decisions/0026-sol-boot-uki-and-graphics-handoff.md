# ADR-0026: SOL boot execution and seamless graphics handoff

- **Status:** Accepted (implementation in progress; UEFI and hardware validation pending)
- **Date:** 2026-08-24
- **Target phase:** Phase 7
- **Extends:** ADR-0019

## Context

ADR-0019 fixes the SOL-owned trust boundary, redundant boot and recovery
authorities, signed A/B deployments, bounded trials, and known-good fallback.
It deliberately leaves the executable boot encoding and firmware integration
open.

SOL also intends to avoid the conventional visible sequence of a low-resolution
firmware screen, a cleared kernel console, another Kernel Mode Setting (KMS)
transition, and a final compositor modeset. The first SOL-owned frame should be
rendered at the display's preferred resolution when firmware makes that mode
available, and it should remain visually continuous while ownership passes from
firmware to early userspace and the compositor.

UEFI Graphics Output Protocol (GOP) exposes only firmware-supported modes and a
simple framebuffer. It does not permit `sol-boot` to synthesize arbitrary panel
timings. Calling `SetMode()` clears the visible output. Native Linux DRM drivers
may also need to reinitialize the device or retrain a display link when they
replace the firmware framebuffer, so a bootloader alone cannot promise a
flicker-free transition on unqualified hardware.

The boot policy must remain testable without firmware. Display continuity must
also remain an optional presentation property: a missing or defective graphics
protocol cannot weaken verification, prevent fallback, or make recovery
unreachable.

## Decision

### 1. Separate boot policy from firmware mechanics

SOL implements deployment selection and trial policy as a deterministic core
library before integrating it with UEFI. The core has no dependency on GOP,
filesystems, firmware variables, wall-clock time, or graphical rendering.

The initial implementation is split into:

```text
boot/sol-boot-core/   versioned state, decisions, transitions, invariants
boot/sol-boot/        signed x86-64 UEFI application and hardware adapters
boot/recovery/        independently bootable non-graphical recovery image
boot/tests/           host fault injection, OVMF, and hardware fixtures
```

`sol-boot-core` consumes validated observations and returns an action. The UEFI
adapter performs I/O, verifies the result of every durable mutation, renders
diagnostics, and executes that action. An attempt is durably consumed before
control is transferred to a trial deployment. A boot-success report must bind
the exact slot, generation, and attempt identity before a deployment can be
promoted to known-good.

Boot-authority copies, recovery copies, and system deployment slots use
different strong types. They are not three interchangeable meanings of `A` and
`B`.

### 2. Reuse UKI and the Linux EFI boot path

The first hardware target is x86-64 UEFI. `sol-boot` is a signed UEFI
application built with a maintained upstream UEFI library. It does not
implement a Linux ELF loader, initrd placement, filesystem driver, GPU driver,
or cryptographic primitive.

Each installed deployment provides a slot-specific Unified Kernel Image (UKI)
that contains the Linux EFI stub, kernel, initrd, immutable command line, and
release metadata. `sol-boot` verifies deployment policy and then invokes the
UKI as a UEFI image through firmware boot services.

The installed deployment manifest evolves beyond the current development
format to bind at least:

- the complete UKI digest and byte length;
- the logical kernel and initrd component identities used to compose that UKI;
- the immutable root-image digest and byte length used during staging;
- the dm-verity root hash and slot-specific root identity used at boot;
- architecture, slot, generation, system version, and runtime descriptors.

The manifest signature and UKI signature serve different purposes. The signed
manifest authorizes a complete SOL deployment and its rollback policy. The
UEFI PE signature allows firmware Secure Boot policy to authorize executable
images. Both must be valid when Secure Boot is enforced.

Staging performs full-file verification before committing the inactive slot.
Normal boot does not hash the entire root image. The signed deployment identity
pins dm-verity metadata, and the kernel verifies root blocks as they are read.

The existing `sol-image` format remains a development foundation; it is not
silently reinterpreted as the final UKI-aware installed format. A versioned
schema change and migration fixtures are required.

### 3. Choose the GOP mode from EDID and firmware capabilities

`sol-boot` uses the GOP instance associated with the active firmware console.
If firmware exposes a usable EDID Active protocol for that output, its preferred
timing is the requested physical resolution. The mode-selection policy is:

1. Enumerate all usable GOP modes and reject malformed descriptions.
2. Prefer the exact EDID preferred width and height when GOP exposes it.
3. If the current GOP mode is that exact mode, preserve it and do not call
   `SetMode()`.
4. Otherwise call `SetMode()` at most once, immediately before rendering the
   first complete SOL frame.
5. Without valid EDID, preserve a usable current mode. Do not assume the
   largest advertised mode is native.
6. If the preferred mode is absent, use the current usable mode and record a
   diagnostic. `sol-boot` never programs an unadvertised timing.

Resolution equality alone does not prove that a later DRM mode is identical.
The native driver and compositor compare the complete available mode identity,
including refresh and timing information, before deciding that a modeset can be
avoided.

The first qualified target is the built-in display driven by the integrated
GPU. External displays, docks, mirrored GOP handles, GPU muxes, and discrete
GPUs are best-effort until they have explicit hardware fixtures.

### 4. Render one static, bounded boot frame

The UEFI stage renders a static SOL boot surface. It may update a small bounded
status or error region, but it does not run continuous animation because GOP
does not provide a portable atomic page-flip or vertical-synchronization
contract.

The renderer:

- honors horizontal resolution, vertical resolution, pixel format, bit masks,
  and pixels-per-scan-line independently;
- bounds every write by the reported framebuffer size;
- supports RGB, BGR, and valid bit-mask modes;
- uses GOP block transfer for modes that do not expose a directly writable
  framebuffer;
- composes into a memory buffer before one full-frame transfer where practical;
- derives layout from physical pixels and a small boot-specific scale policy,
  without importing the desktop UI framework into the UEFI binary.

Boot colors, logo geometry, and the release splash asset are generated from one
source shared by `sol-boot`, the UKI/early-userspace splash, and the compositor.
The UEFI executable does not depend on Slint, Wayland, Mesa, or SolUI.

Security and recovery errors remain legible even when doing so breaks visual
continuity. A polished splash never hides a verification failure or blocks a
non-graphical recovery route.

### 5. Preserve the frame through Linux early boot

After rendering, `sol-boot` does not clear the framebuffer or select another
mode before starting the UKI. The target kernel includes the EFI framebuffer
handoff and an early DRM system-framebuffer driver appropriate for the pinned
kernel version. Kernel framebuffer-console takeover is deferred, and routine
kernel or systemd messages are not written to the graphical console.

The initrd contains a small DRM-capable splash owner. The initial implementation
may use a distribution-integrated upstream splash daemon with a SOL theme. A
future `sol-splashd` must satisfy the same ownership and handoff contract before
replacing it.

The early splash either preserves the firmware frame or redraws the same
release asset at the same physical resolution. It stays alive until the native
DRM driver is ready and the compositor has prepared its first complete frame.
The boot graphics handoff is a presentation contract, not durable boot state;
framebuffer physical addresses are not stored in the deployment manifest or
passed to ordinary userspace.

### 6. Make the compositor's first commit mode-preserving

On the real-hardware backend, `sol-compositor` queries the active connector,
CRTC, plane, and mode before initializing its output. When the active mode is
compatible with SOL's preferred mode, it prepares a GBM framebuffer containing
the same splash/background and first attempts an atomic framebuffer replacement
that does not allow a modeset.

Only if the mode-preserving atomic commit is unsupported or rejected may the
compositor perform one explicit fallback modeset. The fallback is logged with
the GPU, connector, firmware, prior mode, requested mode, and reason so the
hardware can be qualified or denied seamless-boot status.

The compositor does not submit an empty or differently colored intermediate
frame. After its first complete frame is visible, it may perform an
interruptible, reduced-motion-aware transition from the boot surface into the
session. Shell startup is not allowed to hold deployment health promotion
indefinitely.

## Boot display sequence

```text
firmware-selected GOP mode
        ↓ preserve, or one EDID/GOP-selected SetMode
sol-boot static SOL frame
        ↓ no clear
UKI / EFI framebuffer
        ↓ deferred graphical-console takeover
initrd splash on system framebuffer or native DRM
        ↓ same mode + same pixels
native DRM driver
        ↓ first atomic framebuffer replacement without modeset when possible
sol-compositor first complete frame
        ↓ optional visual transition
SOL session
```

## Consequences

- SOL gets a small, testable boot policy and reuses upstream Linux and UEFI
  loading conventions.
- The deployment format requires a versioned UKI-aware extension and dm-verity
  identity before it can be called production boot metadata.
- Boot visuals become a cross-component release artifact rather than unrelated
  bootloader, splash-daemon, and compositor themes.
- The kernel version and early graphics configuration become release inputs;
  changing them requires repeating seamless-handoff validation.
- Native resolution is guaranteed only when firmware exposes the display's
  preferred timing as a usable GOP mode.
- Certified hardware may claim seamless SOL boot. Generic x86-64 UEFI hardware
  may fall back to one visible native-driver transition without affecting boot
  integrity or recovery.

## Required tests

### Host tests

- EDID preferred-mode selection, invalid EDID, missing EDID, and a preferred
  timing absent from GOP produce deterministic decisions.
- A matching current mode never calls `SetMode()`; a different supported mode
  calls it no more than once.
- RGB, BGR, bit-mask, block-transfer-only, unusual stride, and truncated
  framebuffer fixtures cannot write out of bounds.
- Golden boot frames cover common 16:9, 16:10, 3:2, HiDPI, and low-resolution
  modes without changing logo geometry or clipping diagnostics.
- Graphics failure never changes slot selection, consumes an extra boot
  attempt, or prevents text/serial recovery.

### OVMF tests

- The selected GOP mode and framebuffer contents survive UKI invocation.
- Quiet kernel boot preserves the SOL frame until the initrd splash owns a DRM
  device.
- Corrupt graphics assets, unavailable GOP, unsupported pixel formats, and
  failed mode changes use a bounded fallback without affecting verified boot.
- Serial traces identify every graphics owner and whether a modeset occurred.

### Hardware release tests

- After the first SOL-owned frame appears, certified hardware shows no
  unintended all-black frame and no display-link retraining through compositor
  takeover.
- The resolution does not change after `sol-boot` selects the preferred GOP
  mode.
- Integrated Intel and AMD targets cover internal-panel cold boot, warm reboot,
  encrypted-volume prompt, failed deployment trial, and recovery.
- External DisplayPort/HDMI, docks, lid state, GPU muxes, and discrete GPUs are
  marked supported, degraded, or unsupported from recorded evidence rather
  than assumed from QEMU.
- A forced mode-preserving atomic-commit failure produces exactly one logged
  fallback modeset and still reaches a usable session.

## Non-claims

This ADR does not claim that every UEFI implementation reports correct EDID,
that every panel's native mode is available through GOP, or that every Linux
DRM driver can inherit firmware scanout without a blank interval. It does not
select final key-enrollment, TPM measurement, revocation, system-image
filesystem, or delta-update formats.

It also does not make graphical boot a prerequisite for recovery. Serial and
text diagnostics remain required for boot and hardware bring-up.

## Related

- [SOL OS Platform Definition](../os-platform.md)
- [SOL boot and system image](../../boot/README.md)
- [UEFI Graphics Output Protocol](https://uefi.org/specs/UEFI/2.11/12_Protocols_Console_Support.html#graphics-output-protocol)
- [UAPI Unified Kernel Image specification](https://uapi-group.org/specifications/specs/unified_kernel_image/)
- [Linux framebuffer-console deferred takeover](https://docs.kernel.org/fb/fbcon.html)
- ADR-0005 (compositor development path)
- ADR-0019 (OS image and boot boundary)
