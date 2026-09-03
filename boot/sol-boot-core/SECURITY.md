# Security Implementation Status

## Overview

This document tracks the security implementation status for SOL boot state authentication and replay protection.

## Current Status: Development Ready

### ✅ Implemented (Phase 0.5)

1. **HMAC-SHA256 Authentication**
   - Full implementation in `src/auth.rs`
   - Authenticates boot state and success reports
   - Detects any tampering with state
   - Uses `hmac = "0.13"` and `sha2 = "0.11"` (no_std compatible)

2. **Replay Protection (Session-level)**
   - Monotonic sequence number validation
   - Rejects old states within a session
   - Prevents duplicate sequence numbers
   - Session-only protection (resets on reboot)

3. **Attempt Nonce Binding**
   - 32-byte unpredictable nonces per boot attempt
   - Success reports must include correct nonce
   - Prevents copying success reports from other attempts
   - Currently deterministic (hash-based) for development

4. **Comprehensive Test Suite**
   - Authentication tampering detection
   - Replay attack prevention
   - Nonce uniqueness validation
   - Success report authentication
   - Round-trip serialization
   - All tests passing

### ⚠️ Development-only Components

**`SoftwareAuthenticatedStorage`** is suitable ONLY for development and testing:

| Feature | Status | Production Ready |
|---------|--------|------------------|
| HMAC authentication | ✅ Implemented | ✅ Yes |
| Sequence validation | ✅ Implemented | ⚠️ Session-only |
| Nonce generation | ✅ Implemented | ⚠️ Deterministic |
| Persistent replay protection | ❌ Not implemented | ❌ No |
| Hardware key protection | ❌ Not implemented | ❌ No |
| Measured boot binding | ❌ Not implemented | ❌ No |

**Security Limitations:**
- ✅ Detects tampering (authentic HMAC required)
- ✅ Detects replay within a session
- ❌ **Cannot prevent replay after reboot** (counter resets)
- ❌ **Keys stored in memory** (not hardware-protected)
- ❌ **No binding to measured boot state**

**DO NOT DEPLOY TO PRODUCTION** without TPM-backed storage.

### 🔒 Production Requirements (Phase 7)

**`TpmAuthenticatedStorage`** (feature = "tpm") MUST implement:

1. **TPM NV Monotonic Counter**
   - `TPM2_NV_Increment` for sequence tracking
   - Survives reboots and power loss
   - Prevents replay attacks permanently
   - Non-volatile index: `0x01C00002` (recommended)

2. **TPM-Sealed HMAC Keys**
   - Keys sealed to PCR measurements
   - Unsealed only in expected boot state
   - Binds authentication to platform integrity
   - PCRs: 0, 2, 4, 7 (boot firmware + bootloader)

3. **Measured Boot Integration**
   - Success reports include TPM PCR digest
   - Validates entire boot chain integrity
   - Prevents replay from compromised states

4. **TPM RNG for Nonces**
   - `TPM2_GetRandom` for cryptographic randomness
   - Replaces deterministic hash-based generation
   - Ensures unpredictability

### 📋 Phase 7 Checklist

When implementing TPM integration:

- [ ] Initialize TPM2 interface in UEFI environment
- [ ] Create or locate NV index for monotonic counter
- [ ] Define policy for NV access (authValue or policy)
- [ ] Implement key sealing to PCRs
- [ ] Add PCR extend operations for measured boot
- [ ] Implement TPM RNG for nonce generation
- [ ] Add error handling for TPM communication failures
- [ ] Test on real hardware with TPM 2.0
- [ ] Validate replay protection across power cycles
- [ ] Security audit by external reviewer

## API Stability

### Public API (Stable)

```rust
pub trait AuthenticatedStorage {
    fn read_state(&mut self) -> Result<Option<AuthenticatedBootState>>;
    fn write_state(&mut self, state: &AuthenticatedBootState) -> Result<()>;
    fn read_report(&mut self) -> Result<Option<AuthenticatedSuccessReport>>;
    fn generate_nonce(&mut self) -> Result<[u8; 32]>;
    fn compute_auth_tag(&self, message: &[u8]) -> [u8; 32];
    fn verify_auth_tag(&self, message: &[u8], tag: &[u8; 32]) -> bool;
}
```

This API is stable and will not break when switching from software to TPM storage.

### Wire Format

**Authenticated State Format:**
```
sequence (8) || slot_a || slot_b || nonce || HMAC-SHA256 (32)
Total: variable (includes HMAC tag)
```

**Authenticated Report Format:**
```
deployment_id (32) || attempt_nonce (32) || measured_boot_hash (32) ||
checkpoints (4) || timestamp_unix (8) || HMAC-SHA256 (32)
Total: 140 bytes
```

These formats are versioned and forward-compatible.

## Migration Path

### Development → Production

1. Keep using `SoftwareAuthenticatedStorage` during Phase 1-6
2. Implement `TpmAuthenticatedStorage` in Phase 7
3. Switch via feature flag: `cargo build --features tpm`
4. No code changes required in adapters (same trait)

### Testing Strategy

- Unit tests use `SoftwareAuthenticatedStorage::new_for_testing()`
- Integration tests can mock TPM interface
- Hardware tests on real TPM 2.0 devices before production

## References

- ADR-0026: UKI Graphics Handoff and Boot State
- TPM 2.0 Library Specification Part 1 (Architecture)
- UEFI Specification 2.10 Section 37 (TCG Protocol)
- NIST SP 800-147B: BIOS Protection Guidelines

## Security Disclosures

Report security issues to: security@viloris.org (when available)

Do not disclose vulnerabilities publicly before coordinated release.