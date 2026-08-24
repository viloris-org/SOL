# sol-boot-core

`sol-boot-core` is the deterministic, firmware-independent deployment policy
from ADR-0019 and ADR-0026. It models A/B deployment trials, bounded attempts,
known-good fallback, recovery selection, and exact success-report binding.

The crate intentionally has no filesystem, UEFI, GOP, clock, rendering, or
cryptography dependency. Its inputs are already-validated deployment
observations. A future `sol-boot` adapter remains responsible for verification,
durable state storage, diagnostics, and UKI execution.

Trial booting is explicitly two-phase: `prepare_boot` returns a
`BootPlan::PersistTrial` containing the next state, and exposes the trial boot
action only through `confirm_persisted`. This keeps the required ordering visible
at the API boundary: consume and durably verify the attempt before transferring
control to the UKI.

## Durable format 1

The crate also defines allocation-free canonical encodings:

- an 84-byte `DurableBootState` envelope with `SOLSTATE` magic, format and
  length fields, a monotonic durable sequence, fixed A/B records, strict zeroed
  reserved bytes, and CRC32 tear detection;
- a 36-byte `BootSuccessReport` payload binding the exact slot, generation, and
  attempt under `SOLBREPT` magic;
- redundant-copy selection that chooses the highest valid sequence, tolerates
  one missing/corrupt/torn copy, and rejects conflicting equal sequences.

CRC32 is only a torn-write and accidental-corruption detector. It is not an
authenticator. The UEFI/early-userspace adapter must authenticate state and
success-report transport according to the final trust design before applying
policy.

`tests/durable_faults.rs` injects failure before writing, at every byte of a
torn write, before sync, after sync, and during read-back. A trial action is
returned only after the exact next envelope is selected from durable storage;
all other outcomes retain a valid known-good deployment and recover either the
old or safely consumed new attempt state.
