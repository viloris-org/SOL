# ADR-0029: Application signing and publisher lineage

- **Status:** Proposed
- **Date:** 2026-08-26
- **Target phase:** Phase 8 (Native application platform)
- **Extends:** ADR-0012 (application identity), ADR-0020 (`.app` package), ADR-0021 (security permissions)

## Context

SOL requires signed `.app` bundles with verified publisher lineage for:
- Durable security identity (App ID + publisher lineage)
- Permission grant inheritance across updates
- Key rotation without losing user trust and grants
- Detection of publisher discontinuity or compromise

ADR-0021 states: "Publisher-key rotation requires a signature-covered continuity proof from the prior lineage or an explicitly trusted publisher-recovery path."

Android APK Signature Scheme v3+ solved similar problems with:
- Per-signer lineage (proof-of-key-rotation)
- Multiple signing keys with rotation history
- Backward compatibility during key transitions
- Per-SDK-version signing enforcement

SOL needs a concrete signing format that supports these requirements.

## Decision

### 1. Signature scheme overview

SOL uses a **signing block scheme** inspired by Android APK Signature Scheme v2/v3, adapted for SOL's `.app` bundle format:

```text
Example.app (directory-based bundle on disk)
├── App.toml                    # manifest (covered by signatures)
├── bin/x86_64-linux/app        # executables
├── lib/*.so                    # libraries
├── resources/                  # assets, icons, localization
├── metadata/                   # SBOM, licenses, provenance
└── .signatures/                # signature block (inspired by APK Signing Block)
    ├── manifest.json           # content digest tree (like v2 digests)
    ├── v2.sig                  # SOL signature scheme v2 block
    └── lineages/               # publisher key rotation proofs (like APK v3)
        ├── 0.bin               # lineage for v2.sig.signers[0]
        └── 1.bin               # lineage for v2.sig.signers[1] (if multi-signer)

When distributed (archive format):
Example.app.tar.zst             # compressed bundle
└── embedded .signatures/       # same structure, verified on extract
```

**Key differences from Android:**
- SOL bundles are directory-based on disk (macOS-style), archived for distribution
- Signature verification happens at install time (extraction), not at read time
- No need for ZIP-specific tricks (APK Signing Block insertion between ZIP sections)
- Simpler format: `.signatures/` directory instead of embedded binary block
- **Multi-signer support**: `lineages/` directory with one file per signer (not single `lineage.bin`)

**Note on lineage files:**
- For **single-signer bundles**, `lineages/0.bin` is **optional** on first signing (no rotation history yet)
- If absent, verifier treats it as implicit single-node lineage `[current_key]`
- After first key rotation, `lineages/0.bin` becomes **required**
- For **multi-signer bundles** (e.g., company merger), each signer MUST have corresponding lineage file

### 2. Signing algorithm and keys

**Supported algorithms** (in preference order):
1. **Ed25519** — preferred for new signers (fast, small signatures)
2. **ECDSA P-256** — compatibility with existing PKI
3. **RSA-4096 with SHA-256** — legacy compatibility only

**Key types:**
- **Publisher key** — identifies the app's publisher across releases
- **Signing key** — actually signs a specific release (may rotate)
- **Platform key** — SOL system components only (separate trust root)

Each signer has:
```rust
pub struct SignerInfo {
    pub public_key: PublicKey,
    pub algorithm: SignatureAlgorithm,
    pub certificate: Option<Certificate>,  // X.509 cert (optional)
    pub common_name: String,                // "Example Inc."
    pub validity: KeyValidity,
}

pub struct KeyValidity {
    pub not_before: SystemTime,
    pub not_after: SystemTime,
}
```

#### 5.3 lineage.bin (Publisher Lineage - APK v3 style)

Binary format (protobuf-encoded, directly inspired by APK Signature Scheme v3):

```protobuf
message PublisherLineage {
  repeated SignerConfig signers = 1;
  uint32 version = 2;  // lineage format version
}

message SignerConfig {
  bytes certificate = 1;                  // X.509 cert or raw public key (32 bytes for Ed25519)
  optional bytes signed_data = 2;         // SignedSignerConfig serialized (absent for last node)
  repeated Signature signatures = 3;      // signature(s) over signed_data (empty for last node)
}

message SignedSignerConfig {
  bytes next_signer_certificate = 1;  // next key in chain
  SignatureAlgorithm algorithm = 2;
  RotationMetadata metadata = 3;
}

message RotationMetadata {
  string reason = 1;      // "key_expiry", "security_upgrade", "compromise_recovery"
  int64 timestamp = 2;    // Unix timestamp in UTC
  string description = 3; // human-readable explanation
}
```

**Example lineage encoding:**

```text
Lineage: Key A (root) → Key B → Key C (current)

signers[0]: SignerConfig {
  certificate: Key A's cert
  signed_data: SignedSignerConfig {
    next_signer_certificate: Key B's cert
    algorithm: ED25519
    metadata: { reason: "key_expiry", timestamp: 1719792000, ... }
  }
  signatures: [Sign(Key A, signed_data)]
}

signers[1]: SignerConfig {
  certificate: Key B's cert
  signed_data: SignedSignerConfig {
    next_signer_certificate: Key C's cert
    algorithm: ED25519
    metadata: { reason: "security_upgrade", timestamp: 1724889600, ... }
  }
  signatures: [Sign(Key B, signed_data)]
}

signers[2]: SignerConfig {
  certificate: Key C's cert
  signed_data: <absent>  // current key, no next signer (field not set)
  signatures: []
}
```

**Verification logic (mirrors APK v3, with DOS protection and circular reference detection):**

```rust
const MAX_LINEAGE_LENGTH: usize = 100;  // DOS protection
const MAX_LINEAGE_VERIFY_TIME_MS: u64 = 100;  // timeout protection

pub fn verify_lineage(lineage: &PublisherLineage) -> Result<VerifiedLineage> {
    // DOS protection: reject excessively long chains
    if lineage.signers.len() > MAX_LINEAGE_LENGTH {
        return Err(LineageError::ChainTooLong {
            length: lineage.signers.len(),
            max: MAX_LINEAGE_LENGTH,
        });
    }
    
    let start = std::time::Instant::now();
    let mut verified_keys = Vec::new();
    let mut seen_keys = HashSet::new();
    
    for i in 0..lineage.signers.len() {
        // Check timeout on every iteration (not just every 10)
        if start.elapsed().as_millis() > MAX_LINEAGE_VERIFY_TIME_MS as u128 {
            return Err(LineageError::VerificationTimeout);
        }
        
        let signer = &lineage.signers[i];
        
        // Extract current key
        let current_key = extract_public_key(&signer.certificate)?;
        
        // Detect duplicate keys (circular reference protection)
        if !seen_keys.insert(current_key.fingerprint()) {
            return Err(LineageError::DuplicateKey { index: i });
        }
        
        if i < lineage.signers.len() - 1 {
            // Not the last key - must sign next key
            let signed = deserialize::<SignedSignerConfig>(
                &signer.signed_data.as_ref()
                    .ok_or(LineageError::MissingSignedData { index: i })?
            )?;
            let next_cert = &signed.next_signer_certificate;
            
            // CRITICAL: Prevent forward references (circular detection)
            let next_key = extract_public_key(next_cert)?;
            if seen_keys.contains(&next_key.fingerprint()) {
                return Err(LineageError::CircularReference { 
                    index: i,
                    next_index: i + 1,
                });
            }
            
            // CRITICAL: Verify signature using algorithm specified in SignedSignerConfig
            verify_signature_with_algorithm(
                &current_key,
                &signer.signatures[0],
                signer.signed_data.as_ref().unwrap(),
                signed.algorithm,  // Explicitly use declared algorithm
            )?;
            
            // Ensure next_cert matches next signer
            if next_cert != &lineage.signers[i + 1].certificate {
                return Err(LineageError::BrokenChain { index: i });
            }
        } else {
            // Last key: must not have signed_data
            if signer.signed_data.is_some() {
                return Err(LineageError::LastNodeHasSignedData);
            }
        }
        
        verified_keys.push(current_key);
    }
    
    Ok(VerifiedLineage {
        root_key: verified_keys[0].clone(),
        current_key: verified_keys[verified_keys.len() - 1].clone(),
        chain: verified_keys,
    })
}
```

**Key rotation semantics:**
1. First key in lineage is the **root publisher key** (establishes identity)
2. Each subsequent key is signed by its predecessor
3. Only the **latest key** can sign new releases
4. **All keys in lineage** can verify old releases (for rollback)
5. Publisher discontinuity = new root key = new security identity

**Example lineage:**
```text
Publisher: "com.example.editor"

Key A (2024-01-01, root) ──signs──> Key B (2025-06-01, current)
                                        │
                                        └──signs──> Key C (2026-08-01, pending)

Update v1.0: signed by Key A → grants tied to (com.example.editor, lineage=[A])
Update v2.0: signed by Key B → inherits grants (com.example.editor, lineage=[A→B])
Update v3.0: signed by Key C → inherits grants (com.example.editor, lineage=[A→B→C])
```

**Discontinuity detection:**
```text
Attacker signs v2.0 with Key X (no lineage proof from A)
  → New security identity: (com.example.editor, lineage=[X])
  → No grant inheritance
  → User must explicitly authorize again
```

### 6. Complete verification flow (integrating v2 + lineage)

```rust
const MAX_LINEAGE_LENGTH: usize = 100;  // DOS protection

pub fn verify_app_bundle(bundle: &AppBundle) -> Result<VerifiedIdentity, SignatureError> {
    // 1. Load signature block
    let sig_dir = bundle.path.join(".signatures");
    let manifest = read_json(&sig_dir.join("manifest.json"))?;
    let v2_sig = read_protobuf::<SolSignatureV2>(&sig_dir.join("v2.sig"))?;
    
    // 2. Verify content integrity (APK v2 style - check all sections)
    verify_manifest_digests(&manifest, bundle)?;
    
    // 3. Verify all signers with their lineages (all-or-nothing)
    let mut verified_signers = Vec::new();
    let mut failed_signers = Vec::new();
    
    for (index, signer) in v2_sig.signers.iter().enumerate() {
        match verify_signer_with_lineage(signer, index, &manifest, &sig_dir) {
            Ok(verified) => verified_signers.push(verified),
            Err(e) => failed_signers.push((index, e)),
        }
    }
    
    // CRITICAL: All signers must be valid (reject if any invalid)
    // This prevents attackers from adding their own signature alongside legitimate ones
    if !failed_signers.is_empty() {
        return Err(SignatureError::InvalidSignersPresent {
            valid_count: verified_signers.len(),
            failed: failed_signers,
        });
    }
    
    if verified_signers.is_empty() {
        return Err(SignatureError::NoValidSigners);
    }
    
    // 4. Optional: check revocation if cache available
    if let Some(cache) = load_revocation_cache()? {
        for verified in &verified_signers {
            let status = cache.check(
                &verified.lineage.current_key,
                verified.signed_at,
            );
            
            if let RevocationStatus::Revoked { reason, replacement } = status {
                return Err(SignatureError::KeyRevoked {
                    key: verified.lineage.current_key.clone(),
                    reason,
                    safe_replacement: replacement,
                });
            }
        }
    }
    
    // 5. Build verified identity (primary = signers[0])
    Ok(VerifiedIdentity {
        app_id: manifest.app_id.clone(),
        version_code: manifest.version_code,  // NEW: monotonic version for replay protection
        publisher_lineage: verified_signers[0].lineage.clone(),
        bundle_hash: compute_bundle_hash(bundle),
        signed_at: verified_signers[0].signed_at,
        all_signers: verified_signers,
    })
}

fn verify_signer_with_lineage(
    signer: &Signer,
    index: usize,
    manifest: &Manifest,
    sig_dir: &Path,
) -> Result<VerifiedSigner, SignatureError> {
    // 1. Verify signer's signature over manifest
    let public_key = verify_signer(signer, manifest)?;
    
    // 2. Load corresponding lineage (may not exist for first signing)
    let lineage_path = sig_dir.join(format!("lineages/{}.bin", index));
    let lineage = if lineage_path.exists() {
        read_protobuf::<PublisherLineage>(&lineage_path)?
    } else {
        // First signing - create implicit single-node lineage
        // Note: lineage file is NOT created during first signing
        // It will be created on first rotate-key operation
        PublisherLineage {
            signers: vec![SignerConfig {
                certificate: signer.public_key.clone(),
                signed_data: None,  // No rotation yet
                signatures: vec![],
            }],
            version: 1,
        }
    };
    
    // 3. Verify lineage chain
    let verified_lineage = verify_lineage(&lineage)?;
    
    // 4. CRITICAL: Check v2.sig public_key matches lineage current_key
    if public_key != verified_lineage.current_key {
        return Err(SignatureError::SignerLineageMismatch {
            signer_index: index,
            v2_key: public_key,
            lineage_key: verified_lineage.current_key,
        });
    }
    
    Ok(VerifiedSigner {
        public_key,
        lineage: verified_lineage,
        signed_at: extract_timestamp(signer)?,
    })
}

fn verify_signer(signer: &Signer, manifest: &Manifest) -> Result<PublicKey, SignatureError> {
    let public_key = PublicKey::from_bytes(&signer.public_key)?;
    let signed_data = deserialize::<SignedData>(&signer.signed_data)?;
    
    // CRITICAL: Check timestamp within validity period (all timestamps are Unix epoch UTC)
    if let Some(validity) = &signer.validity {
        let timestamp = SystemTime::UNIX_EPOCH + Duration::from_secs(signed_data.timestamp as u64);
        
        if timestamp < validity.not_before {
            return Err(SignatureError::KeyNotYetValid {
                timestamp,
                not_before: validity.not_before,
            });
        }
        
        if timestamp > validity.not_after {
            return Err(SignatureError::KeyExpired {
                timestamp,
                not_after: validity.not_after,
            });
        }
    }
    
    // Verify digests match
    if signed_data.manifest_digest != compute_manifest_digest(manifest) {
        return Err(SignatureError::ManifestDigestMismatch);
    }
    
    // Verify signature
    verify_signature(&public_key, &signer.signature, &signer.signed_data)?;
    
    Ok(public_key)
}


fn verify_manifest_digests(manifest: &Manifest, bundle: &AppBundle) -> Result<()> {
    // Verify App.toml
    let app_toml_content = read_file(&bundle.path.join("App.toml"))?;
    let computed_hash = sha256(&app_toml_content);
    if computed_hash != manifest.bundle_sections.app_toml.sha256 {
        return Err(SignatureError::DigestMismatch("App.toml"));
    }
    
    // Verify executables section
    for (path, expected) in &manifest.bundle_sections.executables.entries {
        let content = read_file(&bundle.path.join(path))?;
        let computed = sha256(&content);
        if computed != expected.sha256 {
            return Err(SignatureError::DigestMismatch(path));
        }
    }
    
    // Similarly for libraries, resources...
    // (omitted for brevity)
    
    Ok(())
}

fn verify_signer(signer: &Signer, manifest: &Manifest) -> Result<PublicKey> {
    // Extract public key
    let public_key = PublicKey::from_bytes(&signer.public_key)?;
    
    // Deserialize signed data
    let signed_data = deserialize::<SignedData>(&signer.signed_data)?;
    
    // Verify digests in signed_data match manifest
    if signed_data.manifest_digest != sha256(&serialize_json(manifest)) {
        return Err(SignatureError::ManifestDigestMismatch);
    }
    
    if signed_data.content_digest != manifest.total_content_hash {
        return Err(SignatureError::ContentDigestMismatch);
    }
    
    // Verify signature over signed_data
    for signature in &signer.signatures {
        match signature.algorithm {
            SignatureAlgorithm::ED25519 => {
                ed25519_verify(&public_key, &signature.value, &signer.signed_data)?;
            }
            SignatureAlgorithm::ECDSA_P256_SHA256 => {
                ecdsa_p256_verify(&public_key, &signature.value, &signer.signed_data)?;
            }
            SignatureAlgorithm::RSA_4096_SHA256 => {
                rsa_verify(&public_key, &signature.value, &signer.signed_data)?;
            }
        }
    }
    
    Ok(public_key)
}
```

### 7. Grant inheritance check (integrating with ADR-0021)

```rust
pub fn check_grant_inheritance(
    old_identity: &VerifiedIdentity,
    new_identity: &VerifiedIdentity,
) -> GrantInheritance {
    // Same App ID required
    if old_identity.app_id != new_identity.app_id {
        return GrantInheritance::Discontinuous;
    }
    
    // CRITICAL: Only check primary signer (signers[0]) for lineage continuity
    // This prevents attacks where attacker adds their signature alongside legitimate one
    // Multi-signer scenarios (company merger) must establish lineage continuity
    // through the primary signer's lineage extending to cover all historical roots
    let old_primary = &old_identity.publisher_lineage;
    let new_primary = &new_identity.publisher_lineage;
    
    if lineage_extends(new_primary, old_primary) {
        return GrantInheritance::SameLineage {
            inherited: true,
            old_root: old_primary.root_key.clone(),
            new_current: new_primary.current_key.clone(),
        };
    }
    
    GrantInheritance::Discontinuous
}

fn lineage_extends(new: &VerifiedLineage, old: &VerifiedLineage) -> bool {
    // Root key must match (same publisher identity)
    if new.root_key != old.root_key {
        return false;
    }
    
    // Old chain must be prefix of new chain
    // Example:
    //   old: [A, B, C]
    //   new: [A, B, C, D, E]  → extends ✓
    //   new: [A, X, Y]        → does not extend ✗ (diverges at B)
    
    if new.chain.len() < old.chain.len() {
        return false;  // cannot extend if shorter
    }
    
    for (i, old_key) in old.chain.iter().enumerate() {
        if &new.chain[i] != old_key {
            return false;  // divergence detected
        }
    }
    
    true
}

pub enum GrantInheritance {
    SameLineage {
        inherited: bool,
        old_root: PublicKey,
        new_current: PublicKey,
    },
    Discontinuous,  // new security identity, no grants inherited
}
```

**Integration with `sol-securityd`:**

```rust
// In sol-securityd permission ledger
impl PermissionLedger {
    pub fn handle_app_update(
        &mut self,
        app_id: &AppId,
        old_identity: &VerifiedIdentity,
        new_identity: &VerifiedIdentity,
    ) -> Result<UpdateOutcome> {
        match check_grant_inheritance(old_identity, new_identity) {
            GrantInheritance::SameLineage { .. } => {
                // Preserve durable grants
                self.migrate_grants(app_id, old_identity, new_identity)?;
                
                // Revoke all live handles (ADR-0021 requirement)
                self.revoke_live_handles(app_id, &old_identity.bundle_hash)?;
                
                Ok(UpdateOutcome::GrantsInherited)
            }
            GrantInheritance::Discontinuous => {
                // Publisher discontinuity - new security identity
                self.revoke_all_grants(app_id)?;
                
                Ok(UpdateOutcome::NewSecurityIdentity)
            }
        }
    }
    
    fn migrate_grants(
        &mut self,
        app_id: &AppId,
        old: &VerifiedIdentity,
        new: &VerifiedIdentity,
    ) -> Result<()> {
        // Update durable grants with new bundle hash
        for grant in self.grants.iter_mut() {
            if grant.app_id == app_id 
                && grant.publisher_root == old.publisher_lineage.root_key {
                grant.current_bundle_hash = new.bundle_hash;
                grant.current_key = new.publisher_lineage.current_key;
                // Keep grant.user, grant.capability, grant.scope unchanged
            }
        }
        Ok(())
    }
}
```

### 5. Detailed signature block format

#### 5.1 manifest.json (content digest tree)

Inspired by Android APK v2 digests, covers all bundle content:

```json
{
  "format_version": 2,
  "app_id": "com.example.editor",
  "version": "2.4.1",
  "version_code": 241,
  "bundle_sections": {
    "app_toml": {
      "path": "App.toml",
      "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
      "size": 1024
    },
    "executables": {
      "entries": {
        "bin/x86_64-linux/app": {
          "sha256": "...",
          "size": 2048576
        }
      }
    },
    "libraries": {
      "entries": {
        "lib/libfoo.so.1": { "sha256": "...", "size": 524288 }
      }
    },
    "resources": {
      "entries": {
        "resources/icon.png": { "sha256": "...", "size": 4096 }
      }
    }
  },
  "total_content_hash": "SHA-256 of (app_toml || executables || libraries || resources)"
}
```

**New field: `version_code`** — Monotonically increasing integer for replay attack protection. The installer (sol-packaged) rejects installations where `new_version_code <= installed_version_code`, preventing attackers from downgrading to older signed versions with known vulnerabilities.

**Why section-based?** Inspired by APK v2's "four regions" approach:
- Fast partial verification (check only executables if resources unchanged)
- Clear separation of content types
- Easier incremental updates in future (v4-style)

#### 5.2 v2.sig (SOL Signature Scheme v2 block)

Binary format (protobuf-encoded, inspired by APK Signature Scheme v2):

```protobuf
message SolSignatureV2 {
  repeated Signer signers = 1;
  uint32 min_sol_version = 2;  // minimum SOL OS version required
}

message Signer {
  bytes signed_data = 1;       // SignedData serialized
  repeated Signature signatures = 2;
  bytes public_key = 3;        // raw key bytes (Ed25519: 32 bytes)
  optional bytes certificate = 4;  // X.509 cert (optional)
}

message SignedData {
  string app_id = 1;
  string version = 2;
  uint64 version_code = 3;         // monotonic version for replay protection
  bytes manifest_digest = 4;       // SHA-256 of manifest.json
  bytes content_digest = 5;        // total_content_hash from manifest
  int64 timestamp = 6;             // Unix timestamp in UTC (seconds since epoch)
  repeated Digest additional_digests = 7;  // future: SHA-512, BLAKE3
}

message Signature {
  SignatureAlgorithm algorithm = 1;
  bytes value = 2;             // raw signature bytes
}

enum SignatureAlgorithm {
  ED25519 = 1;
  ECDSA_P256_SHA256 = 2;
  RSA_4096_SHA256 = 3;
}
```

**Verification flow (like APK v2):**
1. Extract all `Signer` entries
2. For each signer:
   - Verify `signature` over `signed_data` using `public_key`
   - Extract digests from `signed_data`
   - Recompute manifest digest and content digest
   - Compare computed vs signed digests
3. At least one valid signer → bundle is authentic

### 6. Repository signing

Repositories sign metadata separately from app bundles:

```text
Repository: "sol-official-store"

repository-metadata.json (signed by repo key):
{
  "packages": [
    {
      "app_id": "com.example.editor",
      "version": "2.4.1",
      "version_code": 241,
      "bundle_hash": "...",
      "publisher_fingerprint": "...",  # first key in lineage
      "release_date": "2026-08-26",
      "revoked": false
    }
  ],
  "signature": "..."
}
```

**Two-layer trust model:**
1. **App bundle signature**: Verifies publisher identity and content integrity (verified independently)
2. **Repository metadata signature**: Repository vouches that this app was reviewed and approved
   - Signature covers metadata.json only (NOT the app bundles themselves)
   - Each app bundle is verified using its own embedded signatures
   - Repository cannot forge app signatures (only publishers have their private keys)

**Trust boundaries:**
- SOL ships with trusted repository public keys (built into OS)
- Repository metadata provides revocation status and additional context
- App bundles are cryptographically verified independently of repository
- Compromise of repository key cannot forge app signatures

### 8. Key rotation scenarios (APK v3 style)

#### Scenario A: Planned rotation (key expiry)
```text
Timeline:
  2024-01-01: Initial release signed with Key A (expires 2025-12-31)
  2025-11-01: Generate Key B, prepare rotation

Step 1: Generate new key
$ sol-bundle keygen --algorithm ed25519 --out publisher-key-b.key

Step 2: Create lineage (Key A signs Key B)
$ sol-bundle rotate-key \
    --old-key publisher-key-a.key \
    --new-key publisher-key-b.key \
    --reason "key_expiry" \
    --description "Key A expires 2025-12-31" \
    --out lineage.bin

  # lineage.bin now contains:
  # [Key A] → [Key B]
  #   signed_data = { next_signer_certificate: Key B, ... }
  #   signatures = Sign(Key A, signed_data)

Step 3: Sign new release with Key B + lineage
$ sol-bundle sign Example.app \
    --key publisher-key-b.key \
    --lineage lineage.bin

Step 4: User update verification
  Old version: signed by Key A
  New version: signed by Key B, lineage=[A→B]
  
  → System verifies:
    - Key B signature valid ✓
    - Lineage: Key A signed Key B ✓
    - Root key (Key A) matches installed app ✓
  → Grant inheritance: ALLOWED
```

#### Scenario B: Emergency rotation (key compromise)

**Option 1: Continuous lineage (if old key is still usable)**
```text
2026-08-15: Key B compromised, but attacker hasn't used it yet

Step 1: Immediately rotate to Key C
$ sol-bundle rotate-key \
    --old-key publisher-key-b.key \  # still have access
    --new-key publisher-key-c.key \
    --reason "compromise_recovery" \
    --emergency \
    --out lineage.bin

  # lineage.bin: [A] → [B] → [C]

Step 2: Push emergency update signed with Key C
$ sol-bundle sign Example.app \
    --key publisher-key-c.key \
    --lineage lineage.bin \
    --force-update  # mark as security update

Step 3: Notify repository to mark Key B releases as "security_advisory"
$ sol-pkg report-compromise \
    --app-id com.example.editor \
    --compromised-key <Key B fingerprint> \
    --after-timestamp 2026-08-15T00:00:00Z

  → Repository metadata updated:
    {
      "app_id": "com.example.editor",
      "security_advisories": [
        {
          "type": "compromised_key",
          "key": "...",
          "revoked_after": "2026-08-15T00:00:00Z",
          "safe_replacement": "<Key C fingerprint>"
        }
      ]
    }

Result:
  - Users can still update (lineage continuous)
  - Repository warns about compromised versions
  - Grants inherited (same root key)
```

**Option 2: Discontinuous lineage (if attack already happened)**
```text
2026-08-15: Key B fully compromised, attacker released malicious v3.0

Step 1: Generate new root key (no lineage continuity possible)
$ sol-bundle keygen --algorithm ed25519 --out publisher-key-x.key

Step 2: Sign with new root (no lineage)
$ sol-bundle sign Example.app --key publisher-key-x.key

  # .signatures/lineage.bin only contains [Key X]
  # No connection to [A→B]

Step 3: Submit to repository with discontinuity proof
$ sol-pkg submit Example.app \
    --discontinuous \
    --reason "key_compromise_attack" \
    --proof ./compromise-evidence.pdf \
    --contact security@example.com

Step 4: Repository fast-track review
  → Repository verifies:
    - Same developer (out-of-band verification)
    - Evidence of compromise
    - New key unrelated to compromised lineage
  → Marks app as "publisher_reboot"

User experience:
  ┌─────────────────────────────────────────────┐
  │  Security Alert                             │
  │                                             │
  │  "Example Editor" has changed its          │
  │  publisher identity due to a security      │
  │  incident. Previous permissions will       │
  │  not carry over.                           │
  │                                             │
  │  Verified by: SOL Repository               │
  │  New publisher key: abc123...              │
  │                                             │
  │  [More Info] [Install Anyway] [Cancel]     │
  └─────────────────────────────────────────────┘

Result:
  - New security identity: (com.example.editor, lineage=[X])
  - NO grant inheritance
  - User must explicitly re-authorize
```

#### Scenario C: Multi-key signing (company merger)

```text
Company A (Key A) merges with Company B (Key B)
Want users who trust either company to accept updates

Step 1: Create dual-signed release
$ sol-bundle sign Example.app \
    --key company-a.key \
    --lineage lineage-a.bin

$ sol-bundle add-signer Example.app \
    --key company-b.key \
    --lineage lineage-b.bin

  # Now .signatures/ contains:
  #   v2.sig (with TWO Signer entries)
  #   lineages/0.bin (Company A's lineage)
  #   lineages/1.bin (Company B's lineage)

Step 2: Verification
  User previously had app signed by Company A
  Update signed by both A and B
  
  → System verifies:
    - Signer 0 (Company A) signature valid ✓
    - Signer 1 (Company B) signature valid ✓
    - Lineage 0: matches old root key (Company A) ✓
    - Lineage 1: independent lineage (Company B) ✓
  → Grant inheritance: ALLOWED (lineage 0 extends)

  Another user previously had app signed by Company B
  Same update
  
  → System verifies:
    - Both signers valid ✓
    - Lineage 1: matches old root key (Company B) ✓
  → Grant inheritance: ALLOWED (lineage 1 extends)
```

**Future convergence (optional):**

After merger stabilizes, create unified lineage where both historical roots point to same current key:

```bash
$ sol-bundle rotate-key \
    --old-key company-a.key \
    --new-key merged-company.key \
    --out lineage-unified-a.bin

$ sol-bundle rotate-key \
    --old-key company-b.key \
    --new-key merged-company.key \
    --out lineage-unified-b.bin

$ sol-bundle sign Example.app \
    --key merged-company.key \
    --lineage lineage-unified-a.bin

$ sol-bundle add-signer Example.app \
    --key merged-company.key \
    --lineage lineage-unified-b.bin

  # Now both lineages converge:
  # Lineage 0: [A] → [M]
  # Lineage 1: [B] → [M]
  # Same current key (M), different historical roots
```

### 8. Revocation mechanism (addressing Issue #8)

**Problem:** Offline verification cannot check real-time revocation status.

**Solution:** Cached revocation metadata + optional online check

```rust
// Revocation cache (synced from repository metadata)
// Located at: /var/lib/sol/security/revocation-cache.json
{
  "last_sync": "2026-08-26T12:00:00Z",
  "sync_interval_hours": 24,
  "entries": [
    {
      "key_fingerprint": "sha256:abc123...",
      "revoked_after": "2026-08-15T00:00:00Z",
      "reason": "key_compromise",
      "safe_replacement": "sha256:def456..."
    }
  ]
}

// Revocation cache state machine
enum CacheState {
    Fresh,       // age < 24h - use without warning
    Stale,       // age 24h-48h - use with warning
    Expired,     // age > 48h - warn loudly, block if policy requires
    Missing,     // no cache file - offline mode, proceed with warning
}

fn get_cache_state(cache: &RevocationCache) -> CacheState {
    let age = SystemTime::now().duration_since(cache.last_sync).unwrap();
    let hours = age.as_secs() / 3600;
    
    match hours {
        0..=23 => CacheState::Fresh,
        24..=47 => CacheState::Stale,
        _ => CacheState::Expired,
    }
}

// Verification flow with revocation check
fn load_revocation_cache() -> Result<Option<RevocationCache>> {
    let path = Path::new("/var/lib/sol/security/revocation-cache.json");
    if !path.exists() {
        return Ok(None);  // Offline mode, no cache available
    }
    
    let cache = read_json::<RevocationCache>(path)?;
    
    // Log cache state but don't fail verification
    match get_cache_state(&cache) {
        CacheState::Fresh => {
            // All good, use cache silently
        }
        CacheState::Stale => {
            eprintln!("Warning: Revocation cache is {} hours old", 
                     cache.age_hours());
        }
        CacheState::Expired => {
            eprintln!("WARNING: Revocation cache is {} hours old (>48h)", 
                     cache.age_hours());
            eprintln!("Run `sol-pkg sync-revocations` to update");
        }
        CacheState::Missing => unreachable!(), // handled by !path.exists()
    }
    
    Ok(Some(cache))
}

enum RevocationStatus {
    Valid,
    Revoked {
        reason: String,
        replacement: Option<PublicKey>,
    },
    Unknown,  // Not in cache, needs online check
}
```

**Cache update strategy:**

1. **Automatic sync** (sol-securityd):
   - Triggers every 24h when network available
   - Downloads latest repository metadata
   - Updates `/var/lib/sol/security/revocation-cache.json`
   - Retries on network failure: exponential backoff up to 6h interval

2. **Manual sync**:
   ```bash
   sol-pkg sync-revocations
   ```

3. **Install-time check**:
   - If cache exists: use it according to state machine
   - If cache stale (24-48h): warn but proceed
   - If cache expired (>48h): warn loudly, proceed unless policy blocks
   - If cache missing + network available: attempt online fetch
   - If cache missing + offline: proceed with warning logged

**User experience:**

```text
# Normal case (cache fresh)
$ sol-pkg install Example.app
✓ Signature valid
✓ Lineage verified
✓ No revocations found
Installing...

# Cache stale but no revocation
$ sol-pkg install Example.app
✓ Signature valid
✓ Lineage verified
⚠ Revocation cache is 36 hours old (last sync: 2026-08-25)
Installing...

# Cache expired
$ sol-pkg install Example.app
✓ Signature valid
✓ Lineage verified
⚠ WARNING: Revocation cache is 72 hours old
  Run `sol-pkg sync-revocations` to update
Installing...

# Key revoked
$ sol-pkg install Example.app
✗ Signature key has been revoked
  Reason: key_compromise
  Revoked after: 2026-08-15T00:00:00Z
  Safe replacement: sha256:def456...
Installation blocked.
```

**Policy:** Revocation check is **advisory** (warns) unless explicitly required by system policy. This balances security with offline usability.

### 9. Comparison with Android APK Signing

| Aspect | Android APK | SOL `.app` | Rationale |
|--------|-------------|------------|-----------|
| **Format** | ZIP with embedded Signing Block | Directory + `.signatures/` folder | Simpler, no ZIP manipulation |
| **Signature schemes** | v1 (JAR), v2, v3, v4 | v2 (based on APK v2/v3) | Start modern, no legacy |
| **Lineage** | APK Signature Scheme v3 | Same concept, adapted format | Core continuity mechanism |
| **Default algorithm** | RSA-2048 | **Ed25519** | Faster, smaller, more secure |
| **Content protection** | 4 APK regions | Section-based (app_toml, executables, libs, resources) | Clearer structure |
| **Streaming verification** | v4 (Merkle tree + fs-verity) | **Not in Phase 8** | Desktop doesn't need it yet |
| **Per-SDK versioning** | Yes (min_sdk per rotation) | **No** | SOL has no fragmentation |
| **Multi-signer** | Yes | Yes | Enterprise scenarios |
| **Repository signing** | Implicit (Play Store) | **Explicit** | Multiple repo support |
| **Revocation** | Limited | Repository metadata | Better infrastructure |
| **Backward compat** | Must support v1 | None needed | Fresh start |

### 10. Why not use Android's exact format?

**Considered:** Directly reuse APK Signature Scheme v2/v3/v4 binary format

**Rejected because:**

1. **ZIP dependency**: APK signing is deeply tied to ZIP format
   - Signing Block must be inserted between ZIP entries and Central Directory
   - Requires modifying ZIP offsets and EOCD records
   - SOL bundles are directories on disk (macOS-style `.app`)

2. **Unnecessary complexity**:
   - v1 (JAR signing) compatibility not needed
   - v4 (streaming) too complex for Phase 8 desktop needs
   - Per-SDK-version flags irrelevant to SOL

3. **Algorithm modernization**:
   - Android defaults to RSA-2048 (2008 decision)
   - SOL can default to Ed25519 (2024 standard)

4. **Cleaner abstractions**:
   - `.signatures/` directory is more inspectable than binary block
   - Protobuf for structured data, JSON for human-readable manifest
   - Easier to extend without breaking existing tooling

**What we DO borrow from Android:**

✅ **Lineage concept** (APK v3) - core key rotation mechanism  
✅ **Section-based verification** (APK v2) - avoid per-file overhead  
✅ **Multi-signer support** - enterprise use cases  
✅ **Signature-over-digests** - two-level verification  

### 11. Implementation roadmap

#### Phase 8.1: Basic signing (4-6 weeks)

**Deliverables:**
- [ ] `sol-bundle` crate: signature data structures
- [ ] Ed25519 signing implementation (using `ed25519-dalek`)
- [ ] ECDSA P-256 signing (using `p256` crate)
- [ ] RSA-4096 verification only (using `rsa` crate)
- [ ] manifest.json generation (content digest tree)
- [ ] v2.sig generation and verification
- [ ] CLI: `sol-bundle sign`, `sol-bundle verify`
- [ ] Unit tests: sign/verify round-trip, tamper detection

**Success criteria:**
```bash
$ sol-bundle keygen --out publisher.key
$ sol-bundle sign Example.app --key publisher.key
✓ Signed Example.app with Ed25519 (64 bytes)

$ sol-bundle verify Example.app
✓ Signature valid
  App ID: com.example.editor
  Signed by: abc123... (Ed25519)
  Timestamp: 2026-08-26T10:30:00Z

$ echo "malware" >> Example.app/bin/x86_64-linux/app
$ sol-bundle verify Example.app
✗ Verification failed: digest mismatch in bin/x86_64-linux/app
```

#### Phase 8.2: Lineage support (6-8 weeks)

**Deliverables:**
- [ ] `lineage.bin` protobuf format implementation
- [ ] `sol-bundle rotate-key` command
- [ ] Lineage verification logic
- [ ] Grant inheritance integration with `sol-securityd`
- [ ] `sol-bundle check-inheritance` command
- [ ] Multi-signer support (`sol-bundle add-signer`)
- [ ] Integration tests: key rotation, discontinuity detection

**Success criteria:**
```bash
$ sol-bundle rotate-key \
    --old-key key-a.key \
    --new-key key-b.key \
    --reason key_expiry \
    --out lineage.bin
✓ Created lineage: [A] → [B]

$ sol-bundle sign Example.app --key key-b.key --lineage lineage.bin
✓ Signed with Key B (lineage: 2 keys)

$ sol-bundle verify Example.app --show-lineage
✓ Signature valid
  Root key: abc... (Key A)
  Current key: def... (Key B)
  Lineage: 2 rotations
  Rotation 1: key_expiry (2025-11-01)

$ sol-bundle check-inheritance old-Example.app new-Example.app
✓ Same lineage - grants will be inherited
  Root key matches: abc...
  Old key: abc... (Key A)
  New key: def... (Key B)
```

#### Phase 8.3: Repository integration (4-6 weeks)

**Deliverables:**
- [ ] Repository metadata signing
- [ ] Revocation check integration
- [ ] `sol-pkg` integration (install/update/verify)
- [ ] Discontinuity handling in Software app
- [ ] Security advisory UI (compromised keys)

**Success criteria:**
- Repository signs package metadata
- `sol-pkg install` checks both app and repo signatures
- Compromised key releases show warnings
- Discontinuous updates require explicit user consent

#### Phase 9+: Advanced features (deferred)

**Possible future work:**
- [ ] Hardware key support (Yubikey, TPM)
- [ ] Transparency log (public audit of key rotations)
- [ ] Streaming verification (APK v4 style, if large apps emerge)
- [ ] Threshold signatures (require M-of-N keys)
- [ ] Post-quantum algorithms (when standardized)

## Consequences

### Positive
- **Key rotation without losing trust**: lineage proves continuity, like APK v3
- **Backward compatibility**: old keys can still verify old releases (rollback support)
- **Compromise recovery**: discontinuous keys force re-authorization
- **Cryptographic agility**: multiple algorithms supported (Ed25519, ECDSA, RSA)
- **Auditable**: lineage history is transparent and inspectable
- **Fast verification**: section-based digests avoid per-file overhead (APK v2 approach)
- **Modern defaults**: Ed25519 is smaller and faster than Android's RSA-2048
- **Simple format**: `.signatures/` directory is easier to inspect than binary blocks

### Negative
- **Lineage bloat**: long rotation history increases bundle size
  - Mitigation: ~1KB per key rotation (acceptable for 10-20 rotations)
  - Android apps with 5+ rotations show negligible impact
- **Complexity**: verification logic is more complex than single-key
  - Tradeoff: complexity in verifier, simplicity for users (seamless updates)
- **Trust anchor risk**: root key compromise still requires full reset
  - Mitigation: hardware keys (Phase 9+), fast repository response
- **No streaming verification**: must download complete bundle before verification
  - Acceptable for Phase 8 (desktop + stable networks + apps < 500MB)
  - Can add APK v4-style streaming in Phase 9+ if needed

### Security properties
1. **Non-repudiation**: signatures bind publisher to specific release (same as APK)
2. **Integrity**: content digests detect tampering (APK v2 full-content protection)
3. **Authenticity**: lineage proves publisher continuity (APK v3 core feature)
4. **Revocability**: compromised keys can be marked revoked (better than Android)
5. **Forward security**: old key compromise doesn't invalidate new signatures (lineage property)

### Comparison to alternatives

**vs. Single static key:**
- ✅ Can rotate keys without losing app identity
- ✅ Compromised key doesn't force permanent app reset
- ⚠️ More complex verification (acceptable tradeoff)

**vs. X.509 CA chains:**
- ✅ No external CA dependency
- ✅ Publisher directly controls lineage
- ✅ No CRL/OCSP infrastructure required
- ⚠️ No third-party vetting (repository provides this)

**vs. Transparency log only:**
- ✅ Works offline (lineage embedded in bundle)
- ✅ Simpler infrastructure for MVP
- ⚠️ Can add transparency log in Phase 9+ as complement

## Alternatives considered

### A. Single static signing key (rejected)
**Pros**: Simple implementation  
**Cons**: Key compromise or expiry forces new app identity; no rotation path  
**Android experience**: This was pre-v3 limitation; caused major pain for developers  
**Decision**: Rejected - key rotation is essential for long-lived apps

### B. X.509 certificate chains only (rejected)
**Pros**: Reuses PKI infrastructure  
**Cons**: 
- Requires CA trust (external dependency)
- CRL/OCSP complexity for revocation
- Doesn't capture publisher continuity semantics well
- Android moved away from this for good reason  

**Decision**: Rejected - lineage is cleaner for our use case; X.509 certs remain optional

### C. Transparent log (like Certificate Transparency) (deferred)
**Pros**: 
- Public audit trail
- Detect misissuance
- Community oversight  

**Cons**: 
- Complex infrastructure (log operators, monitors)
- Privacy concerns (all updates public)
- Not needed for MVP
- Can be added later as complement  

**Android experience**: No native transparency log (relies on Play Store)  
**Decision**: Deferred to Phase 9+ - lineage provides core continuity; transparency log adds public auditability

### D. Per-release signing without lineage (rejected)
**Pros**: Simpler than lineage  
**Cons**: 
- Every update requires re-authorization (terrible UX)
- No way to distinguish legitimate rotation from attack
- Android v2 had this problem  

**Decision**: Rejected - lineage is the proven solution (APK v3)

### E. Blockchain-based signing (rejected)
**Pros**: Decentralized, immutable record  
**Cons**: 
- Massive complexity and infrastructure
- Offline verification impossible
- Slow and expensive
- No proven benefit over lineage + optional transparency log  

**Decision**: Rejected - overengineered for the problem

### F. Direct APK format reuse (rejected)
**Pros**: 
- Proven format
- Can reuse Android tooling
- Well-documented  

**Cons**: 
- Deeply tied to ZIP format (APK Signing Block insertion)
- SOL bundles are directories, not ZIP
- Includes legacy cruft (v1, per-SDK versioning)
- RSA-2048 default is outdated  

**Decision**: Rejected - borrow concepts (lineage, section-based verification), not binary format

## Related work

### Android APK Signing Schemes
- **APK Signature Scheme v1**: JAR signing (2008) - deprecated, security issues
- **APK Signature Scheme v2**: Full-content signing (2016) - fixed v1 issues
- **APK Signature Scheme v3**: Proof-of-rotation / lineage (2018) - **core inspiration**
- **APK Signature Scheme v4**: Streaming verification (2020) - deferred to Phase 9+

**Key lessons from Android:**
- v1→v2 transition: fixing security issues is painful; start modern
- v2→v3 transition: seamless because lineage was anticipated need
- Lineage is essential for long-lived apps (many apps on 5+ year rotation cycles)
- Multi-signer support matters for enterprises (acquisitions, partnerships)

### iOS Code Signing
- Uses X.509 certificates + provisioning profiles
- Apple controls CA (developer certificates)
- Different model: centralized CA vs. SOL's decentralized repositories
- No lineage concept (certificate renewal requires new signing)

### Debian/RPM package signing
- GPG signatures on package metadata
- No built-in key rotation mechanism
- Trust comes from repository, not package lineage
- Works for distros but not for multi-repo app ecosystems

### Docker Content Trust (Notary)
- TUF (The Update Framework) based
- Supports delegated signing
- More complex than needed for SOL
- Good ideas for repository signing (Phase 8.3)

## Open questions

### 1. Lineage size limits
**Question**: Should we cap lineage length (e.g., max 20 rotations)?  
**Android**: No hard limit, but most apps have < 10 rotations  
**Proposal**: 
- No hard limit in Phase 8
- Monitor real-world usage
- If needed, add "lineage compaction" in Phase 9 (prune old keys with explicit continuity proof)

### 2. Hardware key storage
**Question**: Support hardware keys (Yubikey, TPM) in Phase 8?  
**Proposal**: 
- Phase 8: software keys only (ED25519 file)
- Phase 8.3: add hardware key support for system apps
- Phase 9: full hardware key support for all apps

### 3. Emergency key recovery
**Question**: What if publisher loses ALL keys in lineage?  
**Proposal**: Repository-mediated recovery:
1. Publisher proves identity (out-of-band verification)
2. Repository issues "recovery certificate" for new root key
3. Apps show "Publisher identity verified by SOL Repository" prompt
4. User must explicitly approve (similar to discontinuous lineage)

### 4. Timestamp authority
**Question**: Should signatures include RFC 3161 timestamps?  
**Proposal**: 
- Phase 8: signer-asserted timestamps (in SignedData)
- Phase 9: optional RFC 3161 countersignatures for legal/audit needs

### 5. Algorithm deprecation
**Question**: How to deprecate weak algorithms (e.g., RSA-2048)?  
**Proposal**: 
- Lineage rotations can upgrade algorithm (RSA → Ed25519)
- Repository policy can mark algorithms as "deprecated" or "insecure"
- Verifier enforces minimum algorithm strength per SOL version

## Related

- **ADR-0012**: Application identity (App ID format and lifecycle)
- **ADR-0020**: `.app` package and runtime (bundle format and versioning)
- **ADR-0021**: Default-deny security (permission grants tied to lineage)
- **ADR-0022**: System-managed accounts (grant inheritance with account scopes)
- [OS Platform Definition](../os-platform.md) §5-7 (Application security and permissions)
- [Android APK Signature Scheme v2](https://source.android.com/docs/security/features/apksigning/v2)
- [Android APK Signature Scheme v3](https://source.android.com/docs/security/features/apksigning/v3) - **Primary inspiration**
- [Android APK Signature Scheme v4](https://source.android.com/docs/security/features/apksigning/v4)
- [Android APK Signing Analysis](../android-apk-signing-analysis.md) - **Detailed technical comparison**
- [AOSP apksig Library](https://android.googlesource.com/platform/tools/apksig/)

## Document history

- **2026-08-26**: Initial proposal
  - Lineage concept borrowed from Android APK Signature Scheme v3
  - Section-based verification inspired by APK v2
  - Ed25519 as default algorithm (modern upgrade from Android's RSA-2048)
  - `.signatures/` directory format (adapted from APK Signing Block)
  - Three-phase implementation plan (8.1→8.2→8.3)

## Required tests

### Basic signing and verification
1. ✅ Sign bundle with Ed25519 and verify successfully
2. ✅ Sign bundle with ECDSA P-256 and verify successfully
3. ✅ Verify bundle signed with RSA-4096 (verification only, no signing)
4. ❌ Detect tampered content (modified binary after signing)
5. ❌ Detect tampered manifest (modified manifest.json after signing)
6. ❌ Detect tampered signature (modified v2.sig after signing)
7. ❌ Reject expired signing keys
8. ❌ Reject not-yet-valid signing keys
9. ✅ Multi-signer: verify bundle with 2 valid signatures
10. ❌ Multi-signer: reject if all signers invalid
11. ❌ Multi-signer: reject if ANY signer invalid (all-or-nothing)

### Lineage and key rotation
12. ✅ Single rotation: A→B, verify lineage continuity
13. ✅ Three rotations: A→B→C→D, verify full chain
14. ❌ Detect broken lineage (A→B but B doesn't sign C)
15. ❌ Detect discontinuous lineage (new root key)
16. ❌ Reject signer not in lineage (Key X signs, but lineage is [A→B→C])
17. ✅ Verify old release with old key after rotation (rollback scenario)
18. ❌ Reject lineage with circular reference
19. ❌ Reject lineage with duplicate keys

### Grant inheritance (integration with sol-securityd)
20. ✅ Same-lineage update: inherit durable grants
21. ✅ Same-lineage update: revoke live handles (ADR-0021 requirement)
22. ❌ Discontinuous update: revoke all grants, require new consent
23. ❌ App ID change: treat as different app (no inheritance)
24. ✅ New capability in updated version: remains ungranted
25. ❌ Publisher key compromise scenario: discontinuous lineage prevents inheritance

### Attack resistance
26. ❌ Attacker modifies binary: verification fails
27. ❌ Attacker replaces signature: verification fails (key mismatch)
28. ❌ Attacker creates fake lineage [X→Y]: rejected (root key mismatch)
29. ❌ Attacker steals current key but not lineage: cannot create valid lineage
30. ❌ Replay attack: old signed bundle rejected (version_code check)
31. ❌ Downgrade attack: installer rejects version_code <= installed_version_code
32. ❌ Multi-signer attack: attacker adds their signature alongside legitimate one (all-or-nothing verification blocks this)

### Performance benchmarks (must meet)
33. ✅ Sign 10MB bundle: < 100ms (Ed25519)
34. ✅ Verify 10MB bundle: < 50ms
35. ✅ Verify 100MB bundle: < 200ms
36. ✅ Verify 1GB bundle: < 2s
37. ✅ Build lineage (rotate key): < 50ms
38. ✅ Verify lineage (10 rotations): < 10ms
39. ✅ Verify lineage (100 rotations, DOS limit): < 100ms (timeout enforced)

### Repository integration (Phase 8.3)
40. ❌ Repository signs package metadata
41. ❌ Verify both app signature and repository signature
42. ❌ Detect compromised key via repository metadata
43. ❌ Show security advisory for revoked key
44. ❌ Discontinuous update with repository verification: show trust prompt

### Edge cases
45. ❌ Empty bundle (no executables): reject
46. ❌ Bundle with only resources (no code): allow with warning
47. ❌ Signature timestamp in future: reject (tolerance: 5 minutes for clock skew)
48. ❌ Multiple signatures with different timestamps: use earliest for validity check
49. ❌ Lineage rotation within 1 second: ensure ordering by sequence not timestamp
50. ❌ Maximum lineage depth: reject if > 100 rotations (DOS protection)
51. ❌ Circular lineage reference (A→B→C→B): reject during verification
52. ❌ Lineage verification timeout (>100ms): reject (DOS protection)
53. ❌ Missing lineage file on first signing: create implicit single-node lineage
54. ❌ Revocation cache states (Fresh/Stale/Expired/Missing): handle all gracefully

## Tooling

### sol-bundle CLI

```bash
# Key management
sol-bundle keygen --algorithm ed25519 --out publisher.key
sol-bundle keygen --algorithm ecdsa-p256 --out publisher.key
sol-bundle inspect-key publisher.key

# Signing
sol-bundle sign Example.app --key publisher.key
sol-bundle sign Example.app --key pub.key --timestamp "2026-08-26T10:30:00Z"

# Multi-signer
sol-bundle sign Example.app --key company-a.key
sol-bundle add-signer Example.app --key company-b.key

# Key rotation
sol-bundle rotate-key \
    --old-key publisher.key \
    --new-key publisher-2.key \
    --reason "key_expiry" \
    --description "Old key expires 2025-12-31" \
    --out lineage.bin

# Sign with rotated key
sol-bundle sign Example.app --key publisher-2.key --lineage lineage.bin

# Verification
sol-bundle verify Example.app
sol-bundle verify Example.app --show-lineage
sol-bundle verify Example.app --verbose  # show all digests

# Lineage inspection
sol-bundle inspect-lineage lineage.bin
# Output:
#   Root key: abc123... (Ed25519)
#   Current key: def456... (Ed25519)
#   Rotations: 2
#     1. key_expiry (2025-11-01): Old key expires 2025-12-31
#     2. security_upgrade (2026-08-15): Upgraded to Ed25519

# Grant inheritance check
sol-bundle check-inheritance old-Example.app new-Example.app
# Output:
#   ✓ Same lineage - grants will be inherited
#   Root key: abc123...
#   Old version signed by: abc123... (Key A)
#   New version signed by: def456... (Key B)
#   Lineage: [A] → [B]

# Discontinuity detection
sol-bundle check-inheritance old-Example.app new-Example.app
# Output:
#   ✗ Discontinuous lineage - new security identity
#   Old root key: abc123...
#   New root key: xyz789... (different!)
#   Grants will NOT be inherited

# Repository operations (Phase 8.3)
sol-bundle report-compromise \
    --app-id com.example.editor \
    --compromised-key <fingerprint> \
    --evidence ./proof.pdf \
    --contact security@example.com
```

### Rust API

```rust
use sol_bundle::{Signer, Verifier, LineageBuilder};

// Sign a bundle
let key = PrivateKey::from_file("publisher.key")?;
let mut signer = Signer::new(key);
signer.sign_bundle("Example.app")?;

// Verify a bundle
let verifier = Verifier::new();
let identity = verifier.verify_bundle("Example.app")?;
println!("App ID: {}", identity.app_id);
println!("Signed by: {}", identity.publisher_lineage.current_key);

// Create lineage
let old_key = PrivateKey::from_file("key-a.key")?;
let new_key = PrivateKey::from_file("key-b.key")?;
let lineage = LineageBuilder::new()
    .rotate(old_key, new_key, "key_expiry")
    .build()?;
lineage.save("lineage.bin")?;

// Check grant inheritance
let old_identity = verifier.verify_bundle("old-Example.app")?;
let new_identity = verifier.verify_bundle("new-Example.app")?;
match check_grant_inheritance(&old_identity, &new_identity) {
    GrantInheritance::SameLineage { .. } => {
        println!("Grants will be inherited");
    }
    GrantInheritance::Discontinuous => {
        println!("New security identity - no inheritance");
    }
}
```
