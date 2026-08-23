# ADR-0024: GTK, Qt, and non-native toolkit compatibility

- **Status:** Accepted (architecture; adapters pending)
- **Date:** 2026-08-22
- **Target phase:** Phase 9
- **Extends:** ADR-0020 through ADR-0023

## Context

SOL needs broad Wayland application compatibility without reintroducing global
dependency resolution, weakening the sandbox, or pretending that an arbitrary
GTK/Qt/Electron UI can be made identical to SolUI by injecting a theme.

Non-native apps still need first-class documents, notifications, accounts,
accessibility, settings, and other system capabilities. They also need a safe
way to participate in SOL's material hierarchy without receiving another
surface's pixels.

## Decision

SOL defines three support levels:

| Level | Application | Contract |
|---|---|---|
| **Native** | SolKit/SolUI | Full component, behavior, motion, material, accessibility, and system-framework contract |
| **Integrated** | GTK/Qt or another toolkit with an official SOL adapter | Full system capabilities plus mapped theme/accessibility/window integration and constrained semantic materials |
| **Compatible** | Generic Wayland `.app` | Standard Wayland and portal compatibility while retaining its own visual/interaction system |

Security, identity, accounts, installation, and update guarantees do not vary by
support level. Every app is a signed `.app`, has the same default-deny sandbox,
and uses the same explicit minimum-scope atomic grants. Bypassing SolKit or
shipping a private runtime gives no additional authority.

GTK, Qt, SDL, Electron, Flutter, and similar runtimes are private bundle
dependencies. An app bundles the exact toolkit, platform plugins, and native
libraries it tested. They do not resolve against mutable host copies or satisfy
another app's dependencies.

Official `sol-gtk`, `sol-qt`, and future adapters map toolkit concepts onto
stable SOL ABI/IPC and portals:

- application identity, lifecycle, activation, background work, and commands;
- file chooser, document handles, drag/drop, clipboard, notifications, and
  device/media portals;
- system-managed account selection and scoped credential broker operations;
- light/dark appearance, text scale, contrast, reduced motion, reduced
  transparency, cursor, fonts, and accessibility state;
- window state, decorations, menus, shortcuts, and system actions;
- semantic material requests where the toolkit surface model can express them.

The toolkit-matching adapter/plugin is bundled with the application. SOL does
not inject a process-global GTK theme engine, Qt platform plugin, preload
library, or arbitrary host module into all apps. The stable endpoint is a
versioned protocol or C-compatible contract, not the adapter's internal ABI.

SOL may define a constrained Wayland extension such as
`sol_material_surface_v1`. A client requests only a semantic role (`Chrome`,
`Panel`, `Floating`, or `Control`) and a bounded region. The compositor decides
whether to render, consolidate, or replace it with a solid fallback. The client
never receives backdrop pixels, blur buffers, or capture authority.

## Visual compatibility boundary

Integrated applications should match system appearance, accessibility,
windowing, and functional material hierarchy. SOL does not promise pixel-level
SolUI consistency for controls owned by another toolkit.

Adapters may map SOL semantic tokens to `GtkSettings`/GTK CSS variables,
`QStyle`/Qt palette roles, or equivalent public toolkit APIs. They must not:

- patch private toolkit internals;
- replace an app's tested runtime with a system copy;
- force raw SolUI geometry onto widgets with incompatible behavior;
- claim full Fluid Material when only opacity or client-side blur exists;
- turn visual integration into a permission or screenshot bypass.

## Consequences

- GTK/Qt apps are first-class for system capabilities and security, not
  necessarily identical in component appearance.
- Their `.app` bundles are larger because they carry private toolkit runtimes;
  that is the accepted cost of dependency isolation.
- Adapter versions can evolve independently of the bundled toolkit while the
  stable SOL endpoint remains compatible.
- Generic Wayland apps remain usable without adopting SOL-specific adapters.
- Flatpak compatibility, if shipped, translates into the same portal/security
  boundary and does not become a privileged alternate path.

## Required tests

- Two GTK/Qt apps with incompatible toolkit versions run simultaneously.
- No adapter loads a host toolkit/plugin outside the bundle allowlist.
- Native, integrated, and compatible apps receive identical denial behavior for
  undeclared, implicit, partial, or revoked authority.
- Same-lineage update/rollback, publisher discontinuity, uninstall, and reinstall
  apply identical durable-grant and fresh-handle rules at every support level.
- Account enumeration and durable credentials remain unavailable without an
  explicit account-scoped grant.
- Reduced motion/transparency, high contrast, text scaling, keyboard access,
  and assistive technology propagate through each official adapter.
- Material requests expose no backdrop pixel data and degrade to the same
  hierarchy-preserving solid surface on unsupported paths.
- Adapter failure leaves the application usable through baseline Wayland/
  portal behavior where the toolkit supports it.

## Non-claims

This ADR does not implement the adapters or Wayland material protocol, promise
support for X11/XWayland, or guarantee that every private toolkit API can map to
a SOL feature.

## Related

- [OS Platform Definition](../os-platform.md)
- ADR-0020 (`.app` and shared runtime)
- ADR-0021 (permissions)
- ADR-0022 (accounts)
- ADR-0023 (fluid material)
- ADR-0025 (global menu/status/Live Capsule integration)
