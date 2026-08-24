# SOL Installer welcome

`sol-installer` is the installation entry page launched by a SOL live session.
It clearly distinguishes the temporary live environment from installation and
offers two exits: start the guided installer, or return to the desktop without
changing the machine.

The current scope is deliberately bounded to the welcome surface and its
renderer-neutral handoff. It does **not** implement disk discovery, partition
writes, encryption provisioning, Secure Boot enrollment, or the Phase 7 image
transaction.

```bash
# Deterministic semantic projection used by CI
cargo run -p sol-installer

# Native Wayland window used by a live image
cargo run -p sol-installer --features native
```

`SOL_RELEASE_NAME` supplies the verified image's user-facing release name.
