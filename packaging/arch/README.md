# Arch packaging

This directory is the source-package foundation for SOL's official Arch
repositories. It produces the currently buildable executable packages from one
versioned SOL source archive:

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
no signed release archive or public license declaration yet. Consequently this
directory does **not** claim that source retrieval, source verification,
signed repositories, AUR publication, or installation has been validated.

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

SOL distributes through pacman rather than Flatpak-first (PRD section 30 and
ADR-0008). The official trust chain is planned around signed `[sol-core]`,
`[sol-apps]`, and `[sol-sdk]` repositories. AUR packages are community
maintained and are not part of that trust chain.

See [ADR-0008](../../docs/decisions/0008-distribution-xwayland-scope.md) and
the [roadmap](../../docs/ROADMAP.md).
