# 1. Early monorepo

- Status: Accepted
- Date: 2026-08-15

## Context

SOL starts with tightly coupled compositor, SDK, shell, and first-party apps. PRD section 39 recommends a monorepo to speed up co-evolution.

## Decision

Use a single Rust workspace in this repository. Top-level boundaries follow the PRD:

```text
compositor/ shell/ sdk/ services/ apps/ protocols/ packaging/arch/ tests/ docs/
```

## Consequences

- Cross-cutting changes (compositor <-> SDK <-> app) land atomically.
- Workspace compiles as one unit, making refactors cheaper.
- Boundary discipline is still required so the future `sol-compositor`, `sol-shell`, and `solkit` crates can be split without hidden coupling.
- Re-evaluate after API stabilization (Phase 6).
