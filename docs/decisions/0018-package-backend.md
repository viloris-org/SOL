# ADR-0018: Package backend for SOL applications

- **Status:** Superseded by ADR-0020
- **Date:** 2026-08-16
- **Target phase:** Phase 6

## Context

> Historical decision: the 2026-08-22 OS rebaseline replaced this backend with
> the native `sol-pkg` / `.app` architecture. This file remains as decision
> history and must not be treated as current direction.

The PRD describes pacman and AUR as SOL's system and desktop application
delivery mechanisms. The roadmap leaves a store backend as an optional
decision: a future user-facing store may hide package details, but it must not
invent a second package or trust model.

## Decision

SOL uses Arch package repositories and AUR-compatible packages as the package
backend. The official repositories remain the intended path for SOL-maintained
components; AUR remains an optional community distribution path. A future
graphical store, if built, is only a client of these package mechanisms and
must delegate dependency resolution, signatures, repository trust, and install
transactions to pacman/libalpm.

The project does not create a bespoke package format, background installer, or
store-specific authority. Store metadata may improve discovery, but it cannot
grant capabilities or bypass the system package manager.

## Consequences

- Users and administrators retain one package database and one install model.
- The existing Arch split-package work is the implementation seam for official
  repositories.
- A store UI is optional product work, not a Phase 6 release dependency.
- Signed repository metadata, published archives, and installation testing
  remain required before claiming production packaging.

## Non-claims

This decision does not publish a repository, sign an archive, validate an AUR
submission, or provide a graphical store. It settles only the package-backend
choice.

## Related

- PRD sections 30 and 41 decision #15
- ADR-0008 (distribution scope)
- ADR-0017 (SDK stability and repository review)
