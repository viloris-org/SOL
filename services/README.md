# Services

Independent daemons exposing system capabilities over D-Bus / Wayland
protocols to the compositor, shell, and apps.

## Services

| Service | Path | Responsibility | Status |
|---|---|---|---|
| `sol-settingsd` | `services/sol-settingsd/` | Typed appearance/audio settings backend and `org.sol.Settings1` session-bus adapter | Phase 3; network/display/Bluetooth adapters pending |
| `sol-notificationd` | `services/sol-notificationd/` | Typed notification daemon, `org.sol.Notifications1`, and freedesktop notification interoperability | Phase 4; native surface and real action callback delivery pending |
| `sol-portal` | `services/sol-portal/` | Permission-bound document/capture authorization and `org.sol.Portal1` session-bus adapter | Phase 5 foundation; XDG portal/PipeWire adapters pending |
| `sol-ime` | `services/sol-ime/` | First-party IME frontend + fcitx5 engine bridge | Phase 0 scaffold → Phase 1 |
| `sol-diagnostics` | `services/sol-diagnostics/` | Typed, redacted, bounded local diagnostics store | Phase 5 foundation |
| `sol-ntpd` | `services/sol-ntpd/` | NTPv4/NTS time sampling, authenticated source selection, and bounded privileged clock synchronization | NTS complete; frequency discipline pending |

## Architecture principles

- Services expose their API over D-Bus (parallel to the compositor↔shell
  typed IPC).
- A service crash must not affect the compositor or shell.
- Settings are layered: Settings UI → Settings API (`sol-settingsd`) →
  system services. `org.sol.Settings1` transfers only complete revisioned
  snapshots and named typed mutations; its private persistence format is not a
  client contract.

## See also

- [PRD §26 Settings](../docs/PRD.md#26-settings)
- [PRD §21.1 IME](../docs/PRD.md#211-input-method-ime)
- [ADR-0007 IME decision](../docs/decisions/0007-ime-frontend-fcitx5-engine.md)
- [Roadmap →](../docs/ROADMAP.md)
