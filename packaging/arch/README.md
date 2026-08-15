# Arch packaging

SOL first-party packages ship through Arch repositories, packaged with
**pacman** — not Flatpak-first (PRD §30; ADR-0008).

## Target repositories

| Repo | Contents |
|---|---|
| `[sol-core]` | `sol-compositor`, `sol-shell`, `sol-session`, `sol-settingsd`, `sol-notificationd`, `sol-portal`, `sol-polkit-agent`, `sol-ime`, `sol-desktop` (meta) |
| `[sol-apps]` | `sol-files`, `sol-terminal`, `sol-settings`, `sol-store`, `sol-viewer`, `sol-monitor` |
| `[sol-sdk]` | `solkit`, `sol-ui`, `sol-sdk`, `sol-sdk-docs` |

## Meta package

```bash
sudo pacman -S sol-desktop
```

installs a complete SOL Desktop on a compatible Arch Linux system.

## Notes

- **AUR is not part of SOL's official trust chain.** Official apps ship from
  the signed `[sol-*]` repos; AUR packages are community-maintained.
- A SOL Store (if implemented) hides package-implementation details behind
  pacman/AUR as the real delivery mechanism (PRD §30, §41 #15).
- Functional PKGBUILDs land when the workspace members they reference exist
  (compositor now; shell/SDK/apps as their milestones complete). See
  [roadmap →](../../docs/ROADMAP.md).

## See also

- [ADR-0008 distribution + XWayland scope](../docs/decisions/0008-distribution-xwayland-scope.md)
- [Roadmap →](../../docs/ROADMAP.md)
