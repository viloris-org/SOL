# ADR-0014: Local, privacy-preserving search index

- **Status:** Accepted (Phase 4 foundation)
- **Date:** 2026-08-16
- **Decision:** Start search with a deterministic, in-memory application catalog
  owned by the Shell launcher. It indexes only explicit package/application
  metadata and executes launch results through ADR-0013's typed action and
  permission boundary.

## Context

PRD §28 requires a unified search entry point, while decision #16 asks for a
search-index architecture. An early index that crawls files, documents,
clipboard history, or remote services would create retention, privacy, and
permission questions before the portal and service boundaries exist. Search
also must not become an indirect arbitrary-command launcher.

## Decision

`sol-shell::launcher::LocalSearchIndex` accepts only explicit
`AppCatalogEntry` records keyed by validated `AppId`. The Phase 4 default is
`LocalCatalogOnly`: no filesystem crawl, document content, clipboard, telemetry,
network lookup, or remote ranking.

Matching is case-normalized and deterministic: title exact/prefix matches rank
above package-provided keyword matches, then AppId prefix and substring matches.
Ties are ordered by stable `AppId`. Every result contains only a typed
`SystemAction::LaunchApplication`; `SystemActionApi` authorizes it before a
desktop adapter is allowed to receive the request.

The Shell keeps desktop activation and close as typed `DesktopAction` values,
but the default adapter reports them unavailable. A future compositor/session
transport must provide an explicit adapter and real-session validation; the
recording adapter is a test fixture, not a desktop integration claim.

## Consequences

- Decision #16 is settled with a safe, inspectable initial architecture.
- Application search is useful immediately without collecting personal content.
- Files, documents, settings, commands, calculator, and clipboard providers can
  be added later only through explicit source contracts, ownership, retention,
  permission, and ranking decisions.
- Current headless tests prove ranking and authorization gating, not Wayland
  activation, shell input, file indexing, or system clipboard behavior.

## Related

- PRD §21, §28, §29, §41 decision #16
- ADR-0012 (application identity)
- ADR-0013 (typed System Action and permission layer)
