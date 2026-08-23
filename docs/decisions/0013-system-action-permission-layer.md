# ADR-0013: Typed System Action and Permission Layer

- **Status:** Accepted (Phase 4 API contract; system adapters deferred)
- **Date:** 2026-08-16
- **Decision:** Provide typed, caller-attributed system-action authorization in
  `sol-system`; default to deny; require an explicit, trusted user-consent
  boundary for policy prompts; persist an audit decision before handing an
  authorization to a system adapter.

> **OS rebaseline addendum (2026-08-22):** ADR-0021 keeps this typed API as a
> request/consent foundation and adds the production enforcement boundary:
> authenticated `.app` identity, pre-execution sandbox construction,
> kernel/LSM policy, capability brokers, revocation, and audit. An
> `Authorized` result here is never sufficient on its own to weaken that
> enforcement. `sol-securityd` is the authoritative transaction coordinator;
> same-lineage updates may retain durable grants but receive fresh handles,
> while uninstall/reinstall and publisher discontinuity inherit no authority.

## Context

Search, shell launcher, Quick Settings, notifications, portals, automation,
accessibility tooling, voice, and AI all need to request system work. Giving
those clients a raw command or shell escape hatch makes caller identity,
least-privilege grants, consent, and diagnostics impossible to enforce.

The PRD requires `intent -> typed action -> permission layer -> system service`
and explicitly excludes arbitrary shell execution as the AI default. The same
boundary must remain useful before final IPC and portal adapters are selected.

## Decision

`sol-system` owns a small closed `SystemAction` catalog. The initial catalog
covers launcher application starts, system search, output volume/mute, declared
notification actions, screen capture, and document opening. Each action maps to
a least-privilege `SystemCapability`; it contains no arbitrary executable,
argument vector, or shell string.

Every `SystemActionRequest` carries a validated `sol_app::AppId` and an
`ActionSource`. Grants are keyed by `(AppId, SystemCapability)`, never by a
global boolean. The default `DefaultDenyPolicy` denies all ungranted requests.
A policy may instead return `RequireUserConsent`, which creates an opaque
`ConsentId`; it does not authorize or execute the action. Only trusted system
consent UI may resolve that ID as allow-once, allow-always, or deny.

The service returns only a typed authorization decision. `Authorized` means a
future platform adapter *may* execute the bound request; it does not execute it
itself. Allow, deny, and pending-consent decisions are recorded through an
`ActionAuditStore`. The included memory permission and audit stores are
deterministic fixtures, not a production authority store.

## Consequences

- Search, automation, accessibility, voice, and AI share one constrained,
  auditable authorization vocabulary.
- A caller cannot acquire authority by presenting an untyped command, and a
  consent prompt cannot be replayed after it is resolved.
- Stored deny grants, revoke, and the safe ungranted default are testable
  without compositor or portal infrastructure.
- The catalog will need deliberate additions as system services gain features;
  adding a raw fallback would violate this decision.

## Deferred work and non-claims

This ADR does not implement any system operation or UI. The following still
need real, environment-specific adapters and integration tests:

- `FilePermissionStore` now persists caller/capability allow/deny grants with
  private atomic replacement; durable audit persistence and policy
  administration remain deferred;
- polkit integration and XDG Desktop Portal request/response plumbing;
- concrete shell launcher, search index, Quick Settings, notification, and
  document/capture service adapters;
- Wayland/portal desktop-session validation and AT-SPI/accessibility flows.

The in-memory stores and unit tests prove the contract only; they do not claim
authorization by a running Linux desktop.
