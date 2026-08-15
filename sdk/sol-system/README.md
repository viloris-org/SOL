# sol-system

SOL **Restricted** System API. Exposes a high-level system interface for
first-party apps and SolKit to call on system capabilities (PRD §23 SDK
permission tiers).

## Positioning

In the **Restricted** tier (Public = SolUI/SolApp/SolDocuments/SolGraphics;
Restricted = SolSystem; Private = SolShellKit). First-party "normal" apps
prefer dogfooding the Public API; only components touching real system
permissions use the restricted / private tiers.

## Starting surface area

- Settings read/write proxy
- Notification dispatch
- Screen / recording capability
- Power / Bluetooth / network state
- System actions (reboot / suspend) gated by permission

## Status

**Phase 0 scaffold.** The API converges in Phase 2/3 alongside
Settings / Terminal dogfooding.
