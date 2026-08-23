# SOL OS Platform Definition

**Status:** Normative direction; implementation is pre-alpha
**Baseline:** OS redefinition, 2026-08-22

This document defines the product boundary introduced by the OS rebaseline. If
an older document describes SOL as a desktop layer for Arch Linux, names
pacman/AUR as the native application backend, or conflicts with the security,
account, material, compatibility, or Shell contracts below, this document and
ADR-0019 through ADR-0025 take precedence. The narrower accepted ADR controls
when this overview and an ADR differ in detail.

## 1. Product boundary

SOL is a Linux-kernel operating system. It owns the contracts a user and an
application experience as “the system”:

- boot, recovery, system-image selection, and rollback;
- system construction, signed updates, and hardware enablement;
- application discovery, installation, activation, removal, and rollback;
- application identity, sandboxing, atomic explicit permissions, consent,
  revocation, and audit;
- system-managed local accounts, connected accounts, and credential storage;
- compositor, shell, session, system services, and first-party applications;
- the public SDK and versioned platform runtime used by third-party apps.

SOL may consume upstream kernel, firmware, driver, systemd, Mesa, PipeWire,
NetworkManager, BlueZ, and other projects. Those are implementation inputs, not
the public product boundary. Arch remains useful as an early build source and
developer bootstrap environment; it is no longer the installed-system identity
or application package contract.

## 2. System architecture

```text
UEFI firmware
    ↓
sol-boot ───────────────→ Recovery
    ↓ verify + select
SOL system image A/B
    ↓
Linux kernel + initrd + system services
    ↓
sol-securityd / sol-accountsd / sol-vaultd / sol-packaged / portals
    ↓
SOL Runtime + Wayland compositor + shell
    ↓
signed .app bundles in per-application sandboxes
```

The booted system is split into three update domains coordinated by one signed
deployment record:

1. **Boot authority and recovery** — redundant signed `sol-boot` and recovery
   copies, updated by trial activation with an independently retained fallback.
2. **System deployment** — a slot-bound kernel, initrd, system manifest, and
   versioned read-only root image selected through A/B slots.
3. **Applications** — independently installed, content-addressed `.app`
   bundles activated atomically.

User data and mutable machine state live outside all three domains. Updating or
rolling back executable code must not roll user documents backward.

### Normative terms

| Term | Definition |
|---|---|
| **System deployment** | One signed slot generation binding kernel, initrd, root-image digest, runtime descriptors, and boot metadata; it is the indivisible A/B selection unit. |
| **Known-good** | A signed boot/recovery copy or system deployment that completed its required trial and authenticated health gate and has not been revoked. “Verified” alone is not “known-good.” |
| **Runtime descriptor** | Signed runtime major, monotonic contract revision, and stable feature set exposed by the booted deployment. |
| **Preferred app version** | The signed bundle hash selected by the latest app install/update/manual rollback transaction, plus an ordered retained fallback chain recorded by `sol-packaged`; OS rollback does not rewrite it. |
| **Effective app version** | The first non-revoked compatible bundle in the preferred version's ordered fallback chain for the currently booted runtime descriptor. Version display strings are not used for ordering. |
| **Durable security identity** | Authenticated App ID plus verified publisher lineage; the key used for durable grants. |
| **Release identity** | Durable security identity plus exact bundle hash and process generation; the key used for live handles and leases. |
| **Prepared participant state** | Transaction-ID-bound account/vault state that is neither enumerable nor usable before coordinator commit proof. |
| **Authorization generation** | Monotonic value committed by `sol-securityd`; handles or leases carrying an older generation are invalid even before physical cleanup. |

## 3. Boot and recovery

`sol-boot` is a SOL-owned, signed UEFI bootloader and policy boundary. The EFI
System Partition retains independently addressable current and fallback
`sol-boot` copies. Recovery is likewise redundant. Replacing either authority
is a two-phase trial, never an in-place overwrite of the only bootable copy.

Its minimum contract is:

- load only artifacts accepted by the active trust policy;
- select a complete deployment slot using boot-success and retry metadata;
- fall back automatically after a bounded number of failed boots;
- expose a recovery path that can verify, repair, or reinstall a system slot;
- pass an authenticated system version and slot identity into early userspace;
- never require the graphical shell for recovery.

Each A/B deployment manifest binds the kernel, initrd, root-image digest,
required runtime descriptors, and slot generation. Kernel and initrd are not
updated as free-standing global files: staging a new system release writes the
complete inactive deployment and commits its manifest last.

A `sol-boot` or recovery update follows:

```text
write inactive copy → verify → register one-shot trial → reboot/health gate
                    → promote, or firmware-visible fallback to retained copy
```

The old EFI and recovery copies remain addressable until the new copy has
booted a known-good system and passed the early userspace health gate. Failure
to write firmware variables, loss of power, signature failure, or failure of
the trial copy leaves the previous boot path selected. Garbage collection may
remove a fallback copy only after a newer independent fallback is proven.

The intended chain is:

```text
Platform Secure Boot keys
    → signed sol-boot EFI executable
    → signed deployment manifest binding kernel/initrd/root/runtime/generation
    → measured system identity
    → userspace trust services
```

An early implementation may reuse upstream UEFI libraries and UKI conventions,
but the boot UI, verification policy, slot state machine, rollback behavior, and
recovery contract belong to SOL. “Custom bootloader” does not mean custom
firmware, filesystem drivers, or cryptography.

## 4. Package manager

`sol-pkg` is the user/admin command-line client. `sol-packaged` is the single
privileged transaction service. A graphical Software app is an unprivileged
client of the same API; it is not a second installer.

The manager owns three related transaction types:

| Transaction | Unit | Activation | Rollback |
|---|---|---|---|
| Boot/recovery update | inactive signed EFI/recovery copy + trial record | one-shot trial boot | firmware-visible retained copy |
| System update | slot-bound kernel + initrd + signed manifest + root image | next boot into inactive deployment slot | boot previous known-good deployment |
| App update | signed `.app` bundle | atomic preferred-version switch; effective version resolved at launch | restore a retained preferred version |

Every transaction follows `resolve → fetch → verify → stage → validate →
commit`. Download or validation failure leaves the active system unchanged.
Only one authority may stage mutations to boot/recovery copies, system
deployments, and the machine-wide application store.

Repositories provide signed metadata, hashes, sizes, channels, rollout policy,
revocation state, and transparency information. Trust is rooted in repository
and publisher keys, never in a filename or mutable URL. The package manager
must support offline verification and deterministic inspection before install.

## 5. The `.app` bundle

A `.app` is SOL's native application installation, execution, update, and
identity unit. It is a deterministic, signed, relocatable, read-only bundle;
the installed copy is addressed by its content hash. Users can inspect it as a
bundle, but applications cannot modify their own installed contents.

```text
Example.app/
├── App.toml                 # canonical signed manifest
├── signatures/              # publisher and repository attestations
├── bin/                     # architecture-specific entry points
│   └── x86_64-linux/app
├── lib/                     # private non-SOL shared libraries
├── resources/               # icons, localization, schemas, assets
├── extensions/              # declared extension points
└── metadata/                # licenses, SBOM, provenance
```

The manifest includes at least:

```toml
format = 1
id = "com.example.Editor"
version = "2.4.1"
architectures = ["x86_64"]
entrypoint = "bin/x86_64-linux/app"

[platform]
runtime = "sol-runtime-1"
minimum_contract = 12
required_features = ["documents.v2", "accessibility.tree-v1"]

[capabilities]
requestable = ["documents.open", "notifications.post"]
```

Bundle rules:

- The app vendors every non-SOL userspace dependency it needs under `lib/` or
  its own runtime directory. Those libraries are private and cannot satisfy
  another app's dependencies.
- The app may depend only on a declared SOL Runtime major and explicitly stable
  kernel-facing interfaces. It may not link against arbitrary system-image
  libraries by path.
- `minimum_contract` is a monotonically increasing revision within the declared
  runtime major. `required_features` is an explicit set of stable named ABI/IPC
  capabilities. Both are signature-covered and participate in activation
  resolution; a major name alone is never sufficient compatibility evidence.
- Install scripts and package-time root hooks are forbidden. Registration is
  declarative through the manifest.
- App ID, publisher identity, requestable capabilities, executable hashes,
  extensions, and runtime requirement are signature-covered. Capability
  declaration never grants access.
- App data, cache, configuration, and secrets are stored outside the bundle in
  identity-scoped locations and survive atomic upgrades.
- Side-by-side versions are allowed internally. Each scope/channel has one
  preferred version and, for the current system deployment, at most one effective
  version. An isolated channel has independent pointers and data policy.

This model eliminates cross-application dependency solving. A dependency
change creates a new bundle hash; it never mutates a library beneath another
application.

## 6. SOL Framework Runtime

Self-contained dependencies and a small bundle are compatible because SOL
provides a narrow, versioned platform contract. The rule is:

> Bundle application-specific and third-party dependencies; share only the
> stable SOL platform.

The runtime contains:

- SolUI, SolApp, SolGraphics, SolAnimation, accessibility, localization, and
  document primitives;
- lifecycle, commands, notifications, settings, storage, and background-task
  contracts;
- portal/broker clients for files, devices, media, secrets, sharing, and other
  protected system capabilities;
- account selection, scoped identity, and brokered credential operations backed
  by the system account/vault services;
- language bindings and packaging/build tools supplied by SolKit.

Runtime compatibility is expressed as named major slots such as
`sol-runtime-1`. Every installed runtime publishes a signed descriptor with its
major, monotonically increasing contract revision, and stable feature set.
Compatible additions advance the revision and may add features without removing
older ones; breaking changes ship as a new side-by-side major. Applications do
not bind to unstable internal Rust ABI. In-process framework entry points use a
stable C-compatible ABI where needed; service capabilities use versioned IPC
protocols. SolKit supplies safe Rust and other language bindings over those
boundaries.

An app install, update, or explicit app rollback atomically changes its preferred
bundle hash and records an ordered fallback chain from previously verified,
successfully activated hashes in the same durable security identity and channel.
An update prepends its new hash to the existing chain. An explicit app rollback
selects an existing hash and truncates newer descendants from resolution, though
their content may remain retained; a later user-approved update can prepend a
new release again. A fresh install/reinstall starts a fresh chain.
Application activation is then resolved against the authenticated runtime
descriptor of the currently booted system deployment. `sol-packaged` chooses the
first non-revoked compatible hash in that chain as the effective version; it does
not order releases by their display version string. A system rollback repeats
that resolution but does not rewrite the preferred pointer. It may activate an
older retained compatible app version; returning to a compatible newer system
therefore restores the preferred version automatically. If no compatible version
exists, the app is marked
**temporarily unavailable for this system version**; it does not block boot and
its preferred version and data are not modified.

Garbage collection must retain at least one compatible app version for every
known-good system deployment when such a version has previously been installed.
System-update validation records the compatibility matrix for the candidate and
fallback deployments. The product does not claim that the newest application
release remains runnable after an OS rollback; it guarantees deterministic
selection from the preferred release's ordered fallback chain or an explicit
unavailable state.

An app can fully vendor its own UI/runtime stack when necessary, but doing so
does not grant additional system capabilities. Apps using SOL Runtime should
need only application logic and non-platform libraries in their bundle.

## 7. Application security and permissions

Every third-party `.app` runs as its authenticated App ID inside a default-deny
sandbox. The sandbox is constructed from the signed manifest plus current user
grants; the process cannot broaden it.

Enforcement combines Linux primitives rather than relying on a UI prompt alone:

- mount, PID, IPC, user, and network namespaces as applicable;
- cgroups for resource ownership and limits;
- seccomp for syscall reduction;
- Landlock and/or a selected LSM for filesystem and object policy;
- Wayland protocol mediation;
- portals and capability brokers for user-mediated resources;
- per-app data directories and Secret Service collections.

Permission rules:

- **Minimum authority:** each request must use the narrowest resource, operation,
  duration, and data exposure capable of completing the action. Broad access is
  rejected when a portal or scoped handle can satisfy the request.
- **Default deny:** absence of a declaration and an explicit grant means no
  access. There are no implied grants from installation, first-party branding,
  account login, prior app versions, or SDK choice.
- **Declaration is not authorization:** a manifest only limits what may be
  requested. Every protected capability requires an explicit user or managed
  system-policy grant before use.
- **Atomic grant:** one permission atom is exactly one user × authenticated app
  identity/publisher lineage × capability × resource scope × duration. Grant,
  audit record, and capability-handle/lease issuance commit as one transaction;
  on any failure none becomes effective.
- **No bundled consent:** unrelated capabilities are never accepted through one
  “Allow all” decision. A multi-step operation may explain its needs together,
  but each permission atom remains independently visible, decidable, and
  revocable.
- **Ask at point of use:** prompts identify the app, publisher, resource,
  purpose, and duration. Install-time “accept everything” prompts are avoided.
- **Least scope:** prefer a selected document, device, or one-time token over
  ambient filesystem/device access.
- **Revocable and auditable:** users can inspect and revoke durable grants;
  security-relevant decisions are recorded with bounded, private logs.
- **No raw privileged escape hatch:** Restricted SolKit APIs expose typed
  operations, not arbitrary commands to a privileged daemon.

Permission identity has two layers:

- **durable security identity:** authenticated App ID plus verified publisher
  lineage;
- **release identity:** the exact signed bundle hash and process generation.

A normal update or rollback preserves durable grants only when the repository
verifies publisher continuity and the App ID is unchanged. Key rotation requires
a signature-covered continuity proof from the old lineage or another explicitly
trusted recovery path. A publisher change without continuity creates a new
security identity and inherits no grants.

Preserving a durable grant never preserves a live handle. Bundle activation
revokes release/process-bound handles and leases; the new process must request a
fresh handle, and the broker revalidates that the capability remains declared,
the grant remains valid, and its resource/duration scope still applies. Newly
declared capabilities receive no grant. Uninstall fences and revokes all live
leases and marks every durable grant for that app identity revoked. Reinstalling
the same App ID therefore requires new consent. App-data retention or deletion
is a separate, explicit uninstall choice and never restores authority.

System components are not ordinary third-party apps. They use separately
signed system identities and explicitly provisioned, narrowly scoped service
policy; “first party” is not itself permission to bypass authentication or
audit.

## 8. System-managed accounts and credentials

SOL, not each application, owns durable account and credential storage.
`sol-accountsd` manages device users, connected service accounts, provider
metadata, account lifecycle, and which apps may request an account.
`sol-vaultd` stores passwords, refresh tokens, passkeys, private keys, and
recovery material in encrypted service-owned storage.

Applications receive opaque `AccountHandle` and scoped credential leases. They
do not receive another app's account data, the vault encryption key, or durable
refresh credentials. Where a protocol permits it, a broker performs token use
or refresh and returns only the operation result or a short-lived audience-
bound token.

Adding an account, choosing an account for an app, increasing service scopes,
exporting a secret, and account recovery are trusted system flows. Each app ×
account × service-scope association is an explicit atomic permission grant
under the rules above. App installation or signing status never attaches an
account automatically.

`sol-securityd` is the sole coordinator and durable ledger for permission
transactions, including account-scoped grants. `sol-accountsd` and `sol-vaultd`
are idempotent participants: they may prepare an association or credential lease
under a transaction ID, but prepared state is externally unusable. After
validation, `sol-securityd` atomically records the grant, audit event, participant
receipts, and monotonic authorization generation, then issues a verifiable commit
proof. Participants activate only against that proof. On restart, uncommitted
preparations abort; committed transactions replay idempotently.

Revocation commits a higher authorization generation in `sol-securityd` before
participant cleanup. Brokers and `sol-vaultd` reject an old generation even if
cleanup is incomplete, so a crash cannot resurrect a credential lease. Trusted
UI reports success only after the coordinator has committed; physical cleanup
may converge afterward without restoring access.

Provider/network offline operation may use previously committed bounded material,
but the local authorization generation must still be validated. If the local
coordinator state is unavailable, credential use fails closed.

Credential storage must be encrypted at rest, bound to the user's authenticated
session, and hardware-backed when TPM-class support exists. A documented
recovery-key path is required; hardware loss must not silently weaken storage
encryption. Account removal transactionally revokes outstanding leases and app
associations before deleting or tombstoning credentials.

## 9. SOL fluid material system

SOL uses an adaptive translucent material language inspired by the physical
depth and continuity associated with liquid-glass interfaces, while retaining
its own visual identity. Glass is functional system chrome, not decoration.

Semantic material roles are `Content`, `Chrome`, `Panel`, `Floating`,
`Control`, `Sidebar`, `Dock`, and `Capsule`. Applications select a role through
SolUI; only `sol-design` supplies blur, tint, saturation, edge light, shadow,
grain, and refraction tokens.
Backdrop sampling and distortion are compositor/renderer effects and never
expose another window's pixels to an application.

Hard rules:

- dense app content is solid by default; translucency communicates navigation,
  controls, separation, or transient elevation;
- large surfaces read thicker through stronger blur/tint and depth, while small
  controls remain lighter and more responsive;
- light glass is not stacked repeatedly on glass; nested layers consolidate or
  fall back to a solid material before legibility degrades;
- text and icons maintain contrast over every allowed backdrop, with system-
  resolved vibrancy rather than app-selected gray values;
- material arrival combines blur, edge response, and scale from the current
  presentation state; it remains interruptible and avoids ornamental looping;
- reduced transparency removes backdrop blur/refraction, high contrast uses
  solid bounded surfaces, and reduced motion replaces spatial materialization
  with a short/static transition;
- low-power, remote-session, unsupported-GPU, and frame-pressure modes preserve
  hierarchy with progressively simpler materials.

The token foundation is implemented in `sol-design`; compositor-backed
sampling, adaptive luminance, refraction, and performance validation remain
Phase 4/9 work.

## 10. Installation layout

The exact on-disk layout may evolve, but its ownership model is fixed:

```text
/System                         read-only active system image
/System/Library/Frameworks      active SOL Runtime slots
/var/lib/sol/apps               machine-wide content-addressed app store
/var/lib/sol/system             slot/update/boot-success state
/var/lib/sol/accounts           service-owned account metadata
/var/lib/sol/vault              encrypted credential records
/Applications                   user-facing machine-wide app projections
~/.local/share/sol/apps         per-user app activation metadata
~/.local/share/sol/app-data     identity-scoped durable app data
~/.cache/sol/apps               identity-scoped disposable caches
```

`/Applications/*.app` entries are projections or handles into the managed
store, not mutable copies managed by each application.

## 11. Compatibility policy

SOL distinguishes native, integrated, and compatible applications:

| Level | UI stack | Guarantee |
|---|---|---|
| Native | SolKit/SolUI | Full SOL components, motion, material, accessibility, and system framework |
| Integrated | GTK/Qt with an official SOL adapter | Full system capabilities plus mapped appearance/accessibility/windowing and constrained materials |
| Compatible | Generic Wayland `.app` | Standard Wayland/portal operation with its own visual and interaction system |

Security, permissions, accounts, installation, updates, and rollback are equal
across all three levels. Non-SolKit code never receives more authority.

- Wayland-native Linux applications target SOL by being repackaged as `.app`
  and declaring requestable capabilities.
- GTK, Qt, SDL, Flutter, Electron, and similar stacks are private bundle
  dependencies. The `.app` includes the toolkit, platform plugins, and native
  libraries it tested; no host toolkit copy or global plugin satisfies them.
- Planned `sol-gtk`, `sol-qt`, and other adapters are bundled at a version that
  matches the app's toolkit. They bridge to stable SOL ABI/IPC for portals,
  notifications, accounts, accessibility, appearance, windowing, and system
  actions. SOL does not inject global themes, preload libraries, or private
  toolkit modules into application processes.
- A constrained compositor protocol may accept semantic `Chrome`, `Panel`,
  `Floating`, or `Control` material requests. The compositor decides rendering
  and fallback; clients never receive backdrop pixels or capture authority.
- Integrated apps should match system appearance and functional hierarchy, but
  pixel-identical SolUI widgets are guaranteed only to native applications.
- Flatpak may be offered through a compatibility subsystem, but it is not the
  native SOL package, identity, or permission model.
- pacman/AUR may remain build inputs and developer-bootstrap tools. They are
  not exposed as the installed OS transaction authority.
- X11/XWayland remains outside the first-class compatibility target unless a
  future security and product review explicitly changes that decision.

## 12. Required acceptance tests

The OS platform cannot be called production-ready until automated and
hardware-backed tests prove:

1. a corrupted or non-booting update falls back to a known-good deployment;
2. an interrupted `sol-boot`, recovery, system, or app update leaves an
   independently bootable/usable retained version selected;
3. a failed EFI/recovery trial, firmware-variable write, or power loss cannot
   remove the last known-good boot and recovery paths;
4. signature, hash, publisher, and revocation failures block activation;
5. two apps can carry incompatible versions of the same library without
   interaction;
6. runtime major/revision/feature resolution selects the first non-revoked
   compatible hash in the preferred release's fallback chain for both the
   current and fallback system deployments;
7. an OS rollback activates a retained compatible app version or an explicit
   unavailable state without blocking boot or changing app data;
8. an explicit app rollback changes/truncates the preferred fallback chain so an
   OS transition cannot silently reactivate a newer rolled-back release;
9. undeclared, implicit, partially committed, or revoked capabilities fail at
   the kernel/broker boundary;
10. a failed grant/audit/association/lease transaction produces no usable
   permission, including after coordinator or participant crash/restart;
11. document/device portals grant only the selected resource and duration;
12. same-lineage updates retain durable grants but receive fresh handles, while
    new capabilities, discontinuous publishers, uninstall, and reinstall inherit
    no authority;
13. an app cannot enumerate or use a system-managed account before an explicit
   account-scoped grant, and receives no durable refresh credential afterward;
14. account removal fences outstanding leases before credential deletion;
15. app rollback changes executable content without rolling back user data;
16. recovery can repair or reinstall a slot without requiring the desktop;
17. permission decisions are attributable, inspectable, and revocable;
18. fluid materials meet contrast and frame-budget gates and become solid under
    reduced-transparency/high-contrast modes without changing hierarchy;
19. GTK/Qt apps carrying incompatible toolkit versions run together without
    host-library or plugin resolution;
20. native, integrated, and compatible apps receive identical permission and
    account denial semantics;
21. toolkit material requests expose no backdrop data and preserve hierarchy
    when reduced to solid fallback;
22. foreground menus/status/capsules cannot be spoofed by another App ID, and
    focus changes replace menu snapshots atomically;
23. broker-authoritative microphone/camera/capture indicators cannot be hidden,
    and their Stop/Revoke actions terminate the underlying session.

## 13. Naming and component map

| Component | Responsibility |
|---|---|
| `sol-boot` | Redundant UEFI entries, verification, deployment selection, recovery handoff |
| `sol-image` | Reproducible slot-bound kernel/initrd/root-image composition and manifests |
| `sol-pkg` | User/admin CLI and inspection tools |
| `sol-packaged` | Privileged boot/recovery/system/app transaction engine |
| `sol-bundle` | `.app` build, lint, sign, verify, and inspect tooling |
| `sol-securityd` | Identity, authoritative grant ledger, transaction coordination, revocation, sandbox policy, and audit |
| `sol-accountsd` | Device users, connected accounts, prepared associations, lifecycle |
| `sol-vaultd` | Encrypted credentials and commit-proof-bound scoped credential leases |
| `sol-portal` | User-mediated capability and resource brokers |
| `sol-shell` | Trusted Dock, global menu, status zones, consent, and Live Capsule |
| `sol-runtime-*` | Side-by-side framework majors with signed contract-revision/feature descriptors |
| SolKit | Source SDK, language bindings, templates, and developer tools |
| `sol-design::material` | Semantic fluid material roles and solid fallbacks |
| `sol-gtk` / `sol-qt` | Bundled toolkit adapters over stable SOL ABI/IPC |
| Live Activity service | Attributed, leased registrations for Shell Live Capsule |

These names describe architectural ownership, not a claim that the components
already exist.
