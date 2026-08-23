# SOL non-native toolkit compatibility

This directory is reserved for the Phase 9 compatibility work defined by
[ADR-0024](../docs/decisions/0024-non-native-toolkit-compatibility.md).

Planned ownership:

```text
compat/
├── gtk/            toolkit-matching bundled GTK adapter
├── qt/             toolkit-matching bundled Qt adapter
├── portal/         generic Wayland/XDG portal compatibility contracts
├── material/       semantic material-role protocol and bindings
├── recipes/        SDL, Electron, Flutter, and other .app packaging recipes
└── tests/          version coexistence, denial parity, a11y, fallback fixtures
```

Boundary rules:

- Toolkit/runtime/plugins are private `.app` dependencies.
- Adapters are bundled with the app; the OS does not inject host modules.
- Stable SOL ABI/IPC is the integration boundary, never toolkit-private ABI.
- Native, integrated, and compatible apps have identical permission, account,
  installation, update, and rollback semantics.
- That equality includes grant continuity: verified same-lineage updates may
  retain durable grants but receive fresh handles; publisher discontinuity and
  uninstall/reinstall inherit nothing.
- Material requests carry semantic roles only and never return backdrop pixels.
- Visual integration is best-effort outside SolUI; system capability access is
  not best-effort and must satisfy the same conformance tests.

There is no adapter or custom Wayland protocol implementation here yet.
