# Transitional Arch packaging

> **OS rebaseline:** this directory is retained for developer bootstrap and
> historical split-package validation. It is not the target SOL OS package
> manager, native application format, or production trust path. The current
> direction is `sol-pkg`/`sol-packaged`, signed system images, and signed
> self-contained `.app` bundles; see
> [OS Platform Definition](../../docs/os-platform.md) and ADR-0020.

This directory was the source-package foundation for the former official Arch
repository direction. It still produces the currently buildable executable
packages from one versioned SOL source archive:

| Repository | Packages currently represented |
|---|---|
| `[sol-core]` | `sol-compositor`, `sol-session`, `sol-shell`, `sol-settingsd`, `sol-notificationd`, `sol-portal`, `sol-ime`, `sol-desktop` |
| `[sol-apps]` | `sol-files`, `sol-terminal`, `sol-settings` |
| `[sol-sdk]` | No package yet; the SolKit crates are not public, versioned SDK artifacts. |

`sol-desktop` is a meta package. Its dependencies are deliberately limited to
the binaries this repository can build today. `sol-session` starts the
`sol-compositor --tty-udev`, typed D-Bus services, and `sol-shell` after
validating its runtime directory. The compositor is the critical process;
shell and service companions restart independently. The package installs
`sol.desktop` in the standard
`/usr/share/wayland-sessions` location for a display manager to invoke. It is
not itself a display-manager or login-manager adapter.
Future polkit integration, desktop entries, services, and applications must be added
to the dependency set only when their install contracts exist.

## Build input contract

`PKGBUILD` consumes a release archive named `sol-<version>.tar.gz` whose top
level directory is `sol-<version>/`. A release job must produce that archive
from the exact tagged source revision, calculate its SHA-256 digest, and
replace the temporary `SKIP` digest before publishing. This keeps the package
recipe independent of an unpublished source host while making the required
release artifact and verification step explicit.

The current workspace repository URL is a deliberate placeholder and there is
no signed release archive yet. The source is licensed under BSD-3-Clause, but
this directory does **not** claim that source retrieval, source verification,
signed repositories, AUR publication, or installation has been validated.

## Isolated local build validation

For a local source-archive build check, run:

```bash
./validate-local-build.sh
```

The script creates a temporary `git archive` of the current `HEAD` with the
required `sol-0.1.0/` prefix, copies the package recipe beside it, and invokes
`makepkg --nodeps --cleanbuild` in that temporary directory. It then verifies
all split package archives, their executable payloads, the installed Wayland
session file, and the intentionally empty `sol-desktop` meta-package payload.
It neither installs packages nor changes pacman configuration.

This is only a local source-archive build proof. It does not provide a
canonical repository URL, published archive/checksum, signing trust chain,
repository publication, or a real pacman installation validation.

For a prepared release archive, run from this directory:

```bash
makepkg --syncdeps --cleanbuild
```

`makepkg` will emit the split packages. Do not use this command against the
repository checkout until a release archive and verified checksum have been
provided.

## Static validation

The following check does not download or build anything:

```bash
./validate-pkgbuild.sh
```

It validates shell syntax, `.SRCINFO` generation, the exact package set, and
the `sol-desktop` dependency contract. The CI-friendly check cannot validate a
release archive that has not yet been published.

## Scope

Pacman packages are useful for current development environments and may remain
inputs to system-image construction. They do not install native `.app` bundles,
construct the application sandbox, or commit production SOL system updates.

See [ADR-0019](../../docs/decisions/0019-os-product-and-boot-boundary.md),
[ADR-0020](../../docs/decisions/0020-sol-package-app-runtime.md), and the
[roadmap](../../docs/ROADMAP.md).
