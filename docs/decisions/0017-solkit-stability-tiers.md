# ADR-0017: SolKit stability policy, SDK tiers, and monorepo review

- **Status:** Accepted (Phase 6 policy)
- **Date:** 2026-08-16
- **Target phase:** Phase 6

## Context

PRD section 23 requires a clear distinction between APIs that every
application can use and APIs that carry desktop privileges. PRD section 41,
decision #8, requires a SolKit ABI/API stability strategy after v0.1. The
early-monorepo decision (ADR-0001) also requires a boundary review after API
stabilization.

The workspace currently develops `sol-design`, `sol-ui`, `sol-app`,
`sol-graphics`, `sol-animation`, and `sol-system` together. Their manifests
are `publish = false`; there is no released `solkit` facade crate, published
crate version, public compatibility baseline, or supported external binary
interface. Calling the current source tree stable would therefore make a
promise with no release or verification mechanism behind it.

## Decision

### Stability contract

SOL makes no source-API or ABI compatibility promise for v0.1 or any current
unpublished workspace crate.

After the first explicitly published post-v0.1 Public SDK release, the Public
SDK follows these rules:

- Public Rust source APIs use semantic versioning. Removing or changing a
  documented public item incompatibly requires the next major version.
- Compatible additions and bug fixes use minor and patch versions. A
  deprecated public item remains available through at least one subsequent
  compatible minor release unless it is unsound, insecure, or impossible to
  support.
- The supported compatibility unit is a coherent SolKit release line. Public
  crates consumed together must declare and test compatible versions; a
  released facade crate may later make that unit easier to consume, but is not
  created by this ADR.
- Rust ABI compatibility is **not** promised across compiler versions,
  targets, dynamic-linking modes, or releases. A future stable native ABI must
  be a separately versioned FFI or protocol contract with its own ADR and
  compatibility tests.

Before making that promise, a release must have a documented public-item
inventory, API-diff/semver review against its previous public release,
changelog and migration notes for breaks, and an external-consumer build test.
Those release controls are acceptance work, not evidence supplied by this
policy decision.

### SDK tiers

The tier describes both intended consumers and dependency direction. A public
signature must not expose a type from a more privileged tier.

| Tier | Scope | Contract |
|---|---|---|
| **Public** | `sol-design`, `sol-ui`, `sol-app`, `sol-graphics`, `sol-animation`, and future document/command/testing APIs | Unprivileged application development surface. Third-party and normal first-party apps use this tier. It may depend only on Public APIs. |
| **Restricted** | `sol-system` and any future security/system capability APIs | Typed, caller-attributed operations that can require authorization. It may depend on Public APIs, must keep permissions explicit, and must not provide an untyped route to private services. Availability to third parties is granted capability by capability, not implied by the crate name. |
| **Private** | compositor, shell, services, runtime adapters, and future `SolShellKit` | SOL implementation APIs. They are not supported for external applications and carry no compatibility promise. Private code may consume Public and Restricted contracts, but its types never enter either tier's public signatures. |

Normal first-party applications dogfood the Public tier. A first-party need is
not by itself sufficient to move an API into Restricted or Private; privileged
capabilities must remain narrow typed boundaries with their own authorization
and audit decisions.

### Repository boundary review

SOL remains a single workspace now. The review found no published Public SDK,
no independent component release cadence, and continuing atomic changes across
the SDK, shell, compositor, services, and first-party apps. Splitting now
would add repository and publication overhead without protecting a stable
consumer contract.

Revisit the split decision when all of these are true:

1. a Public SDK release line has passed the release gates above;
2. the SDK has independent external consumers and a documented support window;
3. the dependency graph enforces Public/Restricted/Private boundaries without
   workspace-only path dependencies; and
4. at least one component needs an independently versioned or independently
   released cadence.

Until then, preserve ADR-0001's directory and crate boundaries inside the
monorepo and use them as the extraction seams rather than creating a premature
multi-repository topology.

## Consequences

- A future external developer receives a source-level semver promise only
  after SOL publishes and validates the corresponding Public SDK release.
- Existing source users must not infer a stable ABI, a published crate, or
  permission to depend on Private APIs from this ADR.
- The release process must add public API inventory, compatibility review, and
  external-consumer validation before it can claim the post-v0.1 promise.
- Restricted APIs remain capability boundaries rather than a convenience
  import for system access.
- The current monorepo remains the integration point, while its explicit
  internal boundaries keep a later split feasible.

## Rejected options

1. **Promise Rust ABI stability.** Rejected: Rust does not provide the ABI
   guarantee required for independently compiled native consumers.
2. **Publish every current crate as Public.** Rejected: it would expose
   privileged and implementation APIs before their contracts and permissions
   are ready.
3. **Split the repository immediately.** Rejected: current path dependencies
   and release behavior are still intentionally coordinated; a split has no
   stable contract to enforce yet.
4. **Let first-party code bypass tiering.** Rejected: dogfooding only works if
   ordinary apps use the same Public API expected of third-party developers.

## Non-claims

This ADR does not publish SolKit, change any crate visibility, introduce a
facade crate, implement API-diff tooling, create an ABI, or grant a system
capability. It also does not declare a v0.1 SDK stable. Those are subsequent
implementation and release milestones.

## Related

- PRD sections 17, 23, 31, 39, and 41 decision #8
- ADR-0001 (early monorepo)
- ADR-0011 (settings API boundary)
- ADR-0013 (typed system-action and permission layer)
