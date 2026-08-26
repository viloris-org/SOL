# ADR-0029 Technical Issues and Fixes

This document addresses 8 technical issues found in ADR-0029's initial design.

## Issue #1: Multi-signer lineage承载方式不清晰

**Problem:** Multi-signer scenarios (e.g., company merger) require TWO independent lineages `[A]` and `[B]`, but the protocol defines a single `lineage.bin` file containing one `PublisherLineage` message.

**Root cause:** The document mixed single-file storage with multi-lineage semantics.

**Fix:** Use `lineages/` directory for multi-signer bundles:

```text
.signatures/
  manifest.json
  v2.sig              # Contains multiple Signer entries
  lineages/
    0.bin             # Company A's lineage [A→A'→A'']
    1.bin             # Company B's lineage [B→B'→B'']
```

**Updated protocol:**

```protobuf
message Signer {
  bytes signed_data = 1;
  repeated Signature signatures = 2;
  bytes public_key = 3;
  optional bytes certificate = 4;
  uint32 lineage_index = 5;  // NEW: index into lineages/ directory
}
```

**Verification logic:**

```rust
fn verify_bundle(bundle: &AppBundle) -> Result<Vec<VerifiedSigner>> {
    let sig_dir = bundle.path.join(".signatures");
    let v2_sig = read_proto::<SolSignatureV2>(&sig_dir.join("v2.sig"))?;
    let manifest = read_json::<Manifest>(&sig_dir.join("manifest.json"))?;
    
    let mut verified_signers = Vec::new();
    
    for signer in &v2_sig.signers {
        // 1. Verify signature
        let public_key = verify_signer(signer, &manifest)?;
        
        // 2. Load corresponding lineage
        let lineage_path = if v2_sig.signers.len() == 1 {
            // Single signer: lineage.bin (backward compat)
            sig_dir.join("lineage.bin")
        } else {
            // Multi-signer: lineages/{index}.bin
            sig_dir.join(format!("lineages/{}.bin", signer.lineage_index))
        };
        
        let lineage = if lineage_path.exists() {
            Some(read_proto::<PublisherLineage>(&lineage_path)?)
        } else {
            None  // First release, no rotations yet
        };
        
        // 3. Verify lineage (if exists)
        if let Some(lineage) = lineage {
            verify_lineage(&lineage, &public_key)?;
        }
        
        verified_signers.push(VerifiedSigner {
            public_key,
            lineage,
            signed_at: signer.timestamp,
        });
    }
    
    Ok(verified_signers)
}
```

**Grant inheritance for multi-signer:**

When checking if update can inherit grants, try each lineage:

```rust
fn can_inherit_grants(
    old: &VerifiedIdentity,
    new: &[VerifiedSigner],
) -> Result<bool> {
    // NEW must have at least one signer whose lineage extends OLD's root
    for new_signer in new {
        if let Some(new_lineage) = &new_signer.lineage {
            if new_lineage.root_key == old.publisher_lineage.root_key {
                return Ok(true);  // Found matching lineage
            }
        }
    }
    
    Ok(false)  // No matching lineage = discontinuous
}
```

---

## Issue #2: lineage.bin 在单密钥场景下的存在性未明确

**Problem:** First release has no key rotations, so no lineage exists. But verification flow unconditionally reads `lineage.bin`.

**Fix:** Make `lineage.bin` optional; treat missing file as "initial release with no rotations":

```rust
fn verify_lineage_if_exists(
    sig_dir: &Path,
    current_key: &PublicKey,
) -> Result<Option<VerifiedLineage>> {
    let lineage_path = sig_dir.join("lineage.bin");
    
    if !lineage_path.exists() {
        // Initial release: no rotations yet
        return Ok(Some(VerifiedLineage {
            root_key: current_key.clone(),
            current_key: current_key.clone(),
            rotation_count: 0,
            nodes: vec![],
        }));
    }
    
    let lineage = read_proto::<PublisherLineage>(&lineage_path)?;
    verify_lineage(&lineage, current_key)?;
    
    Ok(Some(VerifiedLineage {
        root_key: extract_root_key(&lineage)?,
        current_key: current_key.clone(),
        rotation_count: lineage.nodes.len(),
        nodes: lineage.nodes,
    }))
}
```

**Signing behavior:**

```bash
# First release: no lineage file created
$ sol-bundle sign Example.app --key publisher.key
✓ Signed Example.app (no lineage - initial release)

# After rotation: lineage.bin created
$ sol-bundle rotate-key --old-key publisher.key --new-key publisher-2.key --out lineage.bin
$ sol-bundle sign Example.app --key publisher-2.key --lineage lineage.bin
✓ Signed Example.app (lineage: 1 rotation)
```

---

## Issue #3: v2.sig 中的 public_key 与 lineage 当前证书的一致性未检查

**Problem:** If `v2.sig.public_key` differs from `lineage.current_certificate.public_key`, attacker could exploit inconsistency.

**Fix:** Add explicit consistency check:

```rust
fn verify_lineage(
    lineage: &PublisherLineage,
    signer_public_key: &PublicKey,
) -> Result<VerifiedLineage> {
    if lineage.nodes.is_empty() {
        return Err(SignatureError::EmptyLineage);
    }
    
    // CRITICAL: Verify chain continuity
    for i in 0..lineage.nodes.len() - 1 {
        let current = &lineage.nodes[i];
        let next = &lineage.nodes[i + 1];
        
        let current_key = extract_public_key(current)?;
        verify_signature(&current_key, &next.signature, &next.signed_data)?;
    }
    
    // CRITICAL: Verify signer's public_key matches lineage's current key
    let lineage_current_key = extract_public_key(lineage.nodes.last().unwrap())?;
    if lineage_current_key != *signer_public_key {
        return Err(SignatureError::LineageMismatch {
            signer_key: signer_public_key.fingerprint(),
            lineage_key: lineage_current_key.fingerprint(),
        });
    }
    
    Ok(VerifiedLineage {
        root_key: extract_public_key(&lineage.nodes[0])?,
        current_key: lineage_current_key,
        rotation_count: lineage.nodes.len() - 1,
        nodes: lineage.nodes.clone(),
    })
}
```

---

## Issue #4: 签名时间未纳入有效期检查

**Problem:** `SignerInfo` defines `validity.not_before` and `not_after`, but verification doesn't check if `signed_data.timestamp` falls within that range.

**Fix:** Already added in the code at line 353-368, but needs emphasis in protocol spec:

```protobuf
message Signer {
  bytes signed_data = 1;
  repeated Signature signatures = 2;
  bytes public_key = 3;
  optional bytes certificate = 4;
  optional KeyValidity validity = 5;  // MUST be checked
}

message KeyValidity {
  int64 not_before = 1;  // Unix timestamp
  int64 not_after = 2;   // Unix timestamp
}
```

**Verification (see existing code at line 353-368):**

- Extract `signed_data.timestamp`
- If `validity` present: reject if `timestamp < not_before || timestamp > not_after`
- If `validity` absent: allow (perpetual key)

---

## Issue #5: Lineage 验证的签名算法一致性

**Problem:** `SignedSignerConfig` has `algorithm` field, but `verify_lineage` doesn't explicitly use it when verifying predecessor→successor signatures.

**Fix:** Pass algorithm explicitly:

```rust
fn verify_lineage(
    lineage: &PublisherLineage,
    signer_public_key: &PublicKey,
) -> Result<VerifiedLineage> {
    for i in 0..lineage.nodes.len() - 1 {
        let current = &lineage.nodes[i];
        let next = &lineage.nodes[i + 1];
        
        let current_key = extract_public_key(current)?;
        
        // CRITICAL: Use algorithm from the signature, not auto-detect
        let algo = current.signature.algorithm;
        verify_signature_with_algo(
            &current_key,
            &current.signature.value,
            &next.signed_data,
            algo,
        )?;
    }
    
    // ... rest of verification
    Ok(verified_lineage)
}

fn verify_signature_with_algo(
    key: &PublicKey,
    sig_bytes: &[u8],
    message: &[u8],
    algo: SignatureAlgorithm,
) -> Result<()> {
    match algo {
        SignatureAlgorithm::ED25519 => {
            let sig = ed25519_dalek::Signature::from_bytes(sig_bytes)?;
            key.as_ed25519()?.verify(message, &sig)?;
        }
        SignatureAlgorithm::ECDSA_P256_SHA256 => {
            let sig = p256::ecdsa::Signature::from_der(sig_bytes)?;
            key.as_ecdsa_p256()?.verify(message, &sig)?;
        }
        SignatureAlgorithm::RSA_4096_SHA256 => {
            key.as_rsa()?.verify(
                PaddingScheme::PSS,
                &Sha256::digest(message),
                sig_bytes,
            )?;
        }
    }
    Ok(())
}
```

---

## Issue #6: 多签名者场景下"至少一个" valid signer 的安全性

**Problem:** If bundle has multiple signers, verification passes if ANY ONE is valid. Attacker could add their own signature alongside legitimate one.

**Fix:** Enforce **all-or-nothing** policy:

```rust
fn verify_bundle(bundle: &AppBundle) -> Result<Vec<VerifiedSigner>> {
    let v2_sig = read_proto::<SolSignatureV2>(&sig_path)?;
    let manifest = read_json::<Manifest>(&manifest_path)?;
    
    let mut verified_signers = Vec::new();
    let mut any_failed = false;
    
    for signer in &v2_sig.signers {
        match verify_signer(signer, &manifest) {
            Ok(verified) => verified_signers.push(verified),
            Err(e) => {
                eprintln!("Signer {} failed: {}", signer.public_key_fingerprint(), e);
                any_failed = true;
            }
        }
    }
    
    // CRITICAL: All signers must be valid
    if any_failed || verified_signers.is_empty() {
        return Err(SignatureError::InvalidSigners {
            total: v2_sig.signers.len(),
            valid: verified_signers.len(),
        });
    }
    
    Ok(verified_signers)
}
```

**Rationale:** If a publisher adds multiple signers (e.g., company merger), BOTH must remain valid. A malicious third party cannot "hitchhike" by adding their signature.

**Exception:** Repository could maintain allowlists of "known multi-signer bundles" for legitimate cases.

---

## Issue #7: Lineage 链长限制 (DoS 防护)

**Problem:** Malicious insider with valid private key could create 10,000-node lineage chain, causing verification DoS.

**Fix:** Add defensive limits:

```rust
const MAX_LINEAGE_NODES: usize = 100;
const MAX_LINEAGE_VERIFY_TIME_MS: u64 = 100;

fn verify_lineage(
    lineage: &PublisherLineage,
    signer_public_key: &PublicKey,
) -> Result<VerifiedLineage> {
    // CRITICAL: Reject excessively long lineages
    if lineage.nodes.len() > MAX_LINEAGE_NODES {
        return Err(SignatureError::LineageTooLong {
            length: lineage.nodes.len(),
            max: MAX_LINEAGE_NODES,
        });
    }
    
    let start = Instant::now();
    
    // Verify chain
    for i in 0..lineage.nodes.len() - 1 {
        // Check timeout every 10 nodes
        if i % 10 == 0 && start.elapsed().as_millis() > MAX_LINEAGE_VERIFY_TIME_MS as u128 {
            return Err(SignatureError::LineageVerificationTimeout);
        }
        
        let current = &lineage.nodes[i];
        let next = &lineage.nodes[i + 1];
        verify_rotation(current, next)?;
    }
    
    // ... rest of verification
    Ok(verified_lineage)
}
```

**Policy:**
- MAX_LINEAGE_NODES = 100 (covers 100 years of annual rotation)
- MAX_LINEAGE_VERIFY_TIME_MS = 100ms (ensures fast verification)
- These are **hard limits** enforced by verifier

---

## Issue #8: Repository 撤销检查未纳入离线验证流程

**Already addressed in main document Section 8**, but summarized here:

**Solution:** Cached revocation metadata + advisory enforcement

```rust
// Verification flow
fn verify_with_revocation_check(bundle: &AppBundle) -> Result<VerifiedIdentity> {
    // 1. Verify signature (always required)
    let verified = verify_bundle(bundle)?;
    
    // 2. Check revocation cache (advisory)
    if let Some(cache) = load_revocation_cache()? {
        for signer in &verified {
            match check_revocation(&cache, &signer.public_key) {
                RevocationStatus::Revoked { reason, .. } => {
                    return Err(SignatureError::KeyRevoked {
                        key: signer.public_key.fingerprint(),
                        reason,
                    });
                }
                RevocationStatus::Unknown => {
                    eprintln!("Warning: Key {} not in revocation cache", 
                             signer.public_key.fingerprint());
                }
                RevocationStatus::Valid => {}
            }
        }
    } else {
        eprintln!("Warning: No revocation cache available (offline mode)");
    }
    
    Ok(verified.into())
}
```

**Cache sync strategy:**
- Automatic: sol-securityd syncs every 24h
- Manual: `sol-pkg sync-revocations`
- Install-time: check cache, warn if stale (>48h)

**Policy:** Revocation check is **advisory** (warns but allows) unless system policy requires strict enforcement.

---

## Summary of Changes

| Issue | Impact | Fix |
|-------|--------|-----|
| #1 Multi-signer lineage | 🔴 Critical | Use `lineages/` directory |
| #2 Initial release lineage | 🟡 Medium | Make `lineage.bin` optional |
| #3 Key consistency | 🔴 Critical | Check v2.sig.public_key == lineage.current_key |
| #4 Timestamp validity | 🟡 Medium | Enforce `not_before` / `not_after` |
| #5 Lineage signature algo | 🟡 Medium | Use explicit algorithm field |
| #6 Multi-signer security | 🔴 Critical | All-or-nothing verification |
| #7 Lineage length DoS | 🟡 Medium | Hard limits: 100 nodes, 100ms timeout |
| #8 Revocation offline | 🟢 Low | Cached metadata + advisory check |

## Updated Test Requirements

Add tests for these fixes:

```bash
# Issue #1: Multi-signer with independent lineages
49. ✅ Multi-signer: Company A lineage [A→A'], Company B lineage [B→B']
50. ✅ Multi-signer update: inherit if either lineage extends

# Issue #2: Initial release
51. ✅ First release: no lineage.bin, verification succeeds
52. ✅ First update after rotation: lineage.bin created

# Issue #3: Key consistency
53. ❌ v2.sig.public_key != lineage.current_key: reject

# Issue #4: Timestamp validity
54. ❌ Signature timestamp before not_before: reject
55. ❌ Signature timestamp after not_after: reject

# Issue #5: Lineage algo
56. ✅ Lineage rotation from RSA to Ed25519: verify with correct algos

# Issue #6: Multi-signer all-or-nothing
57. ❌ Multi-signer with one invalid: reject entire bundle

# Issue #7: DoS protection
58. ❌ Lineage with 101 nodes: reject
59. ❌ Lineage verification taking >100ms: timeout

# Issue #8: Revocation
60. ❌ Revoked key in cache: reject
61. ✅ Stale cache (>48h): warn but allow
62. ✅ Missing cache + offline: allow with warning
```

## Document History

- **2026-08-26**: Initial fixes for 8 technical issues in ADR-0029
