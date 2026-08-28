# SOL non-native toolkit adapters

This directory is reserved for the explicit Phase 9 adapter work retained from
[ADR-0024](../docs/decisions/0024-non-native-toolkit-compatibility.md).

Planned ownership:

```text
compat/
├── gtk/            toolkit-matching bundled GTK adapter
├── qt/             toolkit-matching bundled Qt adapter
├── portal/         SOL portal/runtime adapter contracts
├── material/       semantic SCP material-role bindings
├── recipes/        SDL, Electron, Flutter, and other .app packaging recipes
└── tests/          version coexistence, denial parity, a11y, fallback fixtures
```

Boundary rules:

- Toolkit/runtime/plugins are private `.app` dependencies.
- Adapters are bundled with the app; the OS does not inject host modules.
- Stable SOL ABI/IPC is the integration boundary, never toolkit-private ABI.
- Native, integrated, and adapted apps have identical permission, account,
  installation, update, and rollback semantics.
- That equality includes grant continuity: verified same-lineage updates may
  retain durable grants but receive fresh handles; publisher discontinuity and
  uninstall/reinstall inherit nothing.
- Material requests carry semantic roles only and never return backdrop pixels.
- Visual integration is best-effort outside SolUI; system capability access is
  not best-effort and must satisfy the same conformance tests.

There is no adapter implementation here yet. This directory must not add an
implicit Wayland/X11 compatibility server; every adapter targets SCP and typed
SOL Runtime APIs explicitly.
