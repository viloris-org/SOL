# sol-boot-core

`sol-boot-core` is the deterministic, firmware-independent deployment selector
foundation from ADR-0019 and ADR-0026. It models the current development A/B
trial, bounded attempts, fallback, recovery request, and exact report binding.
It is not the Stage-0 manager selector or the independent recovery authority.

The crate intentionally has no filesystem, UEFI, GOP, clock, rendering, or
cryptography dependency. Its inputs are already-validated deployment
observations. The `sol-boot` adapter is responsible for verification, durable
state storage, diagnostics, and UKI execution.

It also defines the canonical 168-byte signed deployment record used between
`sol-image` and `sol-boot`: a 104-byte payload binds x86-64 slot/generation and
the complete manifest/UKI lengths and SHA-256 digests, followed by a 64-byte
Ed25519 signature. Cryptographic verification stays in the adapter.

Trial booting is explicitly two-phase: `prepare_boot` returns a
`BootPlan::PersistTrial` containing the next state, and exposes the trial boot
action only through `confirm_persisted`. This keeps the required ordering visible
at the API boundary: consume and durably verify the attempt before transferring
control to the UKI.

## Durable format 1 (development)

The crate defines allocation-free canonical encodings:

- an 84-byte `DurableBootState` envelope with `SOLSTATE` magic, format and
  length fields, a monotonic durable sequence, fixed A/B records, strict zeroed
  reserved bytes, and CRC32 tear detection;
- a 36-byte `BootSuccessReport` payload binding the exact slot, generation, and
  attempt under `SOLBREPT` magic;
- redundant-copy selection that chooses the highest valid sequence, tolerates
  one missing/corrupt/torn copy, and rejects conflicting equal sequences.

**CRC32 is only a torn-write and accidental-corruption detector.** It is not an
authenticator or anti-replay mechanism, and two records on one ESP are not
independent storage failure domains.

## Authenticated format (production)

The `auth` module provides HMAC-SHA256 authenticated state storage with replay
protection for production use:

### Security guarantees

- **Authentication**: HMAC-SHA256 detects any tampering with boot state or success reports
- **Replay protection**: Monotonic sequence numbers prevent replaying old states
- **Attempt binding**: Unpredictable 32-byte nonces bind success reports to specific boot attempts
- **Measured boot**: Success reports bind to TPM PCR values or equivalent platform measurements

### Implementations

**`SoftwareAuthenticatedStorage`** (development/testing):
- ✅ HMAC-SHA256 authentication (detects tampering)
- ✅ Sequence monotonicity validation (detects replay within session)
- ❌ No persistent monotonic counter (replay possible after reboot)
- ❌ No hardware-protected keys
- ❌ No measured boot binding

**`TpmAuthenticatedStorage`** (feature = "tpm", Phase 7):
- Hardware-backed HMAC keys sealed to TPM PCRs
- TPM NV monotonic counters for replay-resistant storage
- Measured boot identity binding
- Production-grade security

### Usage

```rust
use sol_boot_core::{
    AuthenticatedStorage, SoftwareAuthenticatedStorage,
    AuthenticatedBootState, AuthenticatedSuccessReport,
};

// Development/testing
let mut storage = SoftwareAuthenticatedStorage::new(hmac_key);

// Generate unpredictable attempt nonce
let nonce = storage.generate_nonce()?;

// Create authenticated state
let state = AuthenticatedBootState {
    sequence: 1,
    slot_a: Some(deployment_record),
    slot_b: None,
    attempt_nonce: Some(nonce),
    auth_tag: [0; 32], // Computed during serialization
};

// Serialize with authentication
let serialized = storage.serialize_state(&state);

// Verify and deserialize
let verified_state = storage.deserialize_state(&serialized)?;
```

### Production requirements

The production adapter must:
1. Use TPM-backed storage with hardware monotonic counters
2. Authenticate state and health observations before acting on them
3. Bind success reports to unpredictable attempt nonces
4. Include measured boot identity (TPM PCRs) in success reports
5. Enforce replay-resistant rollback index before promotion

See ADR-0026 Section 5 for the complete production security contract.

## Fault injection testing

`tests/durable_faults.rs` injects failure before writing, at every byte of a
torn write, before sync, after sync, and during read-back. A trial action is
returned only after the exact next envelope is selected from durable storage;
all other outcomes retain a valid known-good deployment and recover either the
old or safely consumed new attempt state.
