# sol-scheduler

Privileged Phase 1 scheduling policy for ADR-0029. Applications do not link to
this crate or select their own class.

- `sol-init` provisions the cgroup v2 hierarchy, assigns trusted process
  classes, applies nice/OOM/I/O protection, and contains build tools.
- `sol-compositor` runs its render/present event loop at FIFO priority 2 and
  downgrades it when the frame watchdog trips.
- `sol-shell` runs its UI event loop at FIFO priority 1.
- PipeWire uses FIFO priority 10 for DSP threads through the packaged
  `pipewire/10-sol-scheduling.conf` drop-in. Install it at
  `/usr/share/pipewire/pipewire.conf.d/10-sol-scheduling.conf`.

The runtime degrades explicitly when cgroup delegation, realtime scheduling,
or I/O priority capabilities are unavailable, so unprivileged development and
CI remain usable.
