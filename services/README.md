# Services

Independent daemons exposing system capabilities over D-Bus / Wayland
protocols to the compositor, shell, and apps.

## Services

| Service | Path | Responsibility | Status |
|---|---|---|---|
| `sol-settingsd` | `services/sol-settingsd/` | Typed appearance/audio settings backend and `org.sol.Settings1` session-bus adapter | Phase 3; network/display/Bluetooth adapters pending |
| `sol-notificationd` | `services/sol-notificationd/` | Typed notification daemon and `org.sol.Notifications1` session-bus adapter | Phase 4; native surface and standard interoperability pending |
| `sol-portal` | `services/sol-portal/` | Permission-bound typed document-open / capture request boundary | Phase 5 foundation; D-Bus / PipeWire adapters pending |
| `sol-ime` | `services/sol-ime/` | First-party IME frontend + fcitx5 engine bridge | Phase 0 scaffold → Phase 1 |
| `sol-diagnostics` | `services/sol-diagnostics/` | Typed, redacted, bounded local diagnostics store | Phase 5 foundation |

## Architecture principles

- Services expose their API over D-Bus (parallel to the compositor↔shell
  typed IPC).
- A service crash must not affect the compositor or shell.
- Settings are layered: Settings UI → Settings API (`sol-settingsd`) →
  system services. `org.sol.Settings1` transfers only complete revisioned
  snapshots and named typed mutations; its private persistence format is not a
  client contract.

## See also

- [PRD §26 Settings](../../PRD.md#26-settings)
- [PRD §21.1 IME](../../PRD.md#211-input-method-ime)
- [ADR-0007 IME decision](../docs/decisions/0007-ime-frontend-fcitx5-engine.md)
- [Roadmap →](../ROADMAP.md)
