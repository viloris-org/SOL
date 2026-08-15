# Services

Independent daemons exposing system capabilities over D-Bus / Wayland
protocols to the compositor, shell, and apps.

## Services

| Service | Path | Responsibility | Status |
|---|---|---|---|
| `sol-settingsd` | `services/sol-settingsd/` | Settings backend (Appearance / Displays / Sound / Network / Bluetooth / …) | Phase 0 scaffold → Phase 3 |
| `sol-notificationd` | `services/sol-notificationd/` | Notification daemon | Phase 0 scaffold → Phase 4 |
| `sol-portal` | `services/sol-portal/` | xdg-desktop-portal implementation (file pick / screencast / record / …) | Phase 0 scaffold → Phase 4/5 |
| `sol-ime` | `services/sol-ime/` | First-party IME frontend + fcitx5 engine bridge | Phase 0 scaffold → Phase 1 |

## Architecture principles

- Services expose their API over D-Bus (parallel to the compositor↔shell
  typed IPC).
- A service crash must not affect the compositor or shell.
- Settings are layered: Settings UI → Settings API (`sol-settingsd`) →
  system services.

## See also

- [PRD §26 Settings](../../PRD.md#26-settings)
- [PRD §21.1 IME](../../PRD.md#211-input-method-ime)
- [ADR-0007 IME decision](../docs/decisions/0007-ime-frontend-fcitx5-engine.md)
- [Roadmap →](../ROADMAP.md)
