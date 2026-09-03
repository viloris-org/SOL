//! Authoritative SCP identity, capability, token, and audit coordinator.
//!
//! `sol-securityd` owns the signing key and policy state. The compositor only
//! holds opaque capability tokens and asks this service to verify them.

use nix::{
    sys::socket::{getsockopt, sockopt::PeerCredentials},
    unistd::Uid,
};
use ring::{
    hmac,
    rand::{SecureRandom, SystemRandom},
};
use sol_compositor::scp::{
    capability::{self, Capability},
    security::{
        AuditOutcome, LOCK_SERVICE_APP_ID, SHELL_APP_ID, SecurityRequest, SecurityResponse,
        WireDecision, read_security_frame, write_security_frame,
    },
};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Read, Write},
    os::unix::{
        fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const TOKEN_MAGIC: &[u8; 4] = b"SLTK";
const TOKEN_VERSION: u8 = 1;
const TOKEN_TAG_BYTES: usize = 32;
const TOKEN_NONCE_BYTES: usize = 16;
const TOKEN_FIXED_BYTES: usize = 4 + 1 + 1 + 8 + 8 + 2 + 2 + TOKEN_NONCE_BYTES;
const MAX_TOKEN_FIELD: usize = 1024;
const DEFAULT_TOKEN_LIFETIME: Duration = Duration::from_hours(1);
const SENSITIVE_TOKEN_LIFETIME: Duration = Duration::from_secs(60);
const ACCEPT_POLL: Duration = Duration::from_millis(25);
const MAX_AUDIT_BYTES: u64 = 4 * 1024 * 1024;

/// Daemon paths. Package activation owns the identity registry and Settings or
/// policy administration owns the grant ledger.
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    pub socket_path: PathBuf,
    pub state_dir: PathBuf,
    pub identity_registry: PathBuf,
    pub grants: PathBuf,
    pub trusted_bin_dir: PathBuf,
}

impl SecurityConfig {
    #[must_use]
    pub fn system_default() -> Self {
        let state_dir = std::env::var_os("SOL_SECURITYD_STATE_DIR")
            .map_or_else(|| PathBuf::from("/var/lib/sol/security"), PathBuf::from);
        Self {
            socket_path: std::env::var_os("SOL_SECURITYD_SOCKET")
                .map_or_else(|| PathBuf::from("/run/sol/securityd.sock"), PathBuf::from),
            identity_registry: std::env::var_os("SOL_SECURITYD_IDENTITIES")
                .map_or_else(|| state_dir.join("identities.tsv"), PathBuf::from),
            grants: std::env::var_os("SOL_SECURITYD_GRANTS")
                .map_or_else(|| state_dir.join("grants.tsv"), PathBuf::from),
            trusted_bin_dir: std::env::var_os("SOL_SECURITYD_TRUSTED_BIN_DIR")
                .map_or_else(|| PathBuf::from("/usr/lib/sol"), PathBuf::from),
            state_dir,
        }
    }

    fn key_path(&self) -> PathBuf {
        self.state_dir.join("token.key")
    }

    fn audit_path(&self) -> PathBuf {
        self.state_dir.join("audit.tsv")
    }
}

/// Running policy engine. It is safe to share across connection workers.
pub struct SecurityService {
    config: SecurityConfig,
    signing_key: hmac::Key,
    random: SystemRandom,
    consumed_tokens: Mutex<HashMap<[u8; TOKEN_TAG_BYTES], u64>>,
    next_dialog_id: AtomicU64,
    audit_lock: Mutex<()>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PolicyDecision {
    Allow,
    Deny,
    Prompt,
}

#[derive(Debug, Default)]
struct GrantLedger {
    generation: u64,
    decisions: HashMap<(String, String), PolicyDecision>,
}

#[derive(Debug)]
struct TokenClaims {
    app_id: String,
    capability: String,
}

impl SecurityService {
    /// Open private durable state and load/create the HMAC key.
    ///
    /// # Errors
    ///
    /// Returns an error when the private state directory, signing key, or
    /// consumed-token ledger cannot be opened with secure ownership and modes.
    pub fn open(config: SecurityConfig) -> io::Result<Self> {
        fs::create_dir_all(&config.state_dir)?;
        fs::set_permissions(&config.state_dir, fs::Permissions::from_mode(0o700))?;
        let key = load_or_create_key(&config.key_path())?;
        let consumed_tokens = load_consumed_tokens(&config.state_dir.join("consumed.tsv"))?;
        Ok(Self {
            config,
            signing_key: hmac::Key::new(hmac::HMAC_SHA256, &key),
            random: SystemRandom::new(),
            consumed_tokens: Mutex::new(consumed_tokens),
            next_dialog_id: AtomicU64::new(1),
            audit_lock: Mutex::new(()),
        })
    }

    /// Handle one authenticated compositor request.
    #[must_use]
    pub fn handle(&self, request: SecurityRequest) -> SecurityResponse {
        match request {
            SecurityRequest::VerifyIdentity { pid } => SecurityResponse::Identity {
                app_id: self.verify_identity(pid),
            },
            SecurityRequest::Evaluate { app_id, capability } => SecurityResponse::Decision {
                decision: self.evaluate(&app_id, &capability),
            },
            SecurityRequest::IssueToken { app_id, capability } => {
                let ledger = match self.read_grants() {
                    Ok(ledger) => ledger,
                    Err(error) => return error_response("read grant ledger", &error),
                };
                if Self::policy(&ledger, &app_id, &capability) != PolicyDecision::Allow {
                    return SecurityResponse::Error {
                        message: "capability is not authorized".to_owned(),
                    };
                }
                if let Err(error) =
                    self.append_audit(&app_id, &capability, "granted", "issue-token")
                {
                    return error_response("commit authorization audit", &error);
                }
                self.issue_response(&app_id, &capability, ledger.generation)
            }
            SecurityRequest::VerifyToken { token } => match self.verify_signed_token(&token) {
                Some(claims) => SecurityResponse::Verified {
                    app_id: Some(claims.app_id),
                    capability: Some(claims.capability),
                },
                None => SecurityResponse::Verified {
                    app_id: None,
                    capability: None,
                },
            },
            SecurityRequest::Audit {
                app_id,
                capability,
                outcome,
            } => match self.append_audit(&app_id, &capability, audit_name(outcome), "use") {
                Ok(()) => SecurityResponse::Ack,
                Err(error) => error_response("commit audit", &error),
            },
            // Tokens are stateless and bounded. Release is advisory; revocation
            // is enforced through the ledger generation carried in each token.
            SecurityRequest::ReleaseTokens { .. } => SecurityResponse::Ack,
        }
    }

    fn evaluate(&self, app_id: &str, capability: &str) -> WireDecision {
        if Capability::from_wire_name(capability).is_none() || !valid_app_id(app_id) {
            return WireDecision::Denied {
                reason: "invalid application or capability".to_owned(),
            };
        }
        let ledger = match self.read_grants() {
            Ok(ledger) => ledger,
            Err(error) => {
                tracing::error!(%error, "cannot read grant ledger");
                return WireDecision::Denied {
                    reason: "security policy is unavailable".to_owned(),
                };
            }
        };
        match Self::policy(&ledger, app_id, capability) {
            PolicyDecision::Allow => {
                // Audit durability is part of authorization: no audit, no
                // handle. Token construction after this point is infallible.
                if let Err(error) = self.append_audit(app_id, capability, "granted", "evaluate") {
                    tracing::error!(%error, "cannot commit authorization audit");
                    return WireDecision::Denied {
                        reason: "authorization transaction could not be committed".to_owned(),
                    };
                }
                match self.issue(app_id, capability, ledger.generation) {
                    Ok((token, expiry, one_time)) => WireDecision::Granted {
                        token,
                        expires_at_unix_ms: Some(expiry),
                        one_time,
                    },
                    Err(error) => WireDecision::Denied {
                        reason: format!("token issuance failed: {error}"),
                    },
                }
            }
            PolicyDecision::Deny => {
                let _ = self.append_audit(app_id, capability, "denied", "policy");
                WireDecision::Denied {
                    reason: "denied by sol-securityd policy".to_owned(),
                }
            }
            PolicyDecision::Prompt => WireDecision::NeedsUserConsent {
                dialog_id: self.next_dialog_id.fetch_add(1, Ordering::Relaxed),
            },
        }
    }

    fn issue_response(&self, app_id: &str, capability: &str, generation: u64) -> SecurityResponse {
        match self.issue(app_id, capability, generation) {
            Ok((token, expiry, one_time)) => SecurityResponse::Token {
                token,
                expires_at_unix_ms: Some(expiry),
                one_time,
            },
            Err(error) => error_response("issue token", &error),
        }
    }

    fn issue(
        &self,
        app_id: &str,
        capability: &str,
        generation: u64,
    ) -> io::Result<(Vec<u8>, u64, bool)> {
        if !valid_app_id(app_id)
            || Capability::from_wire_name(capability).is_none()
            || app_id.len() > MAX_TOKEN_FIELD
            || capability.len() > MAX_TOKEN_FIELD
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid token scope",
            ));
        }
        let parsed = Capability::from_wire_name(capability)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unknown capability"))?;
        let one_time = matches!(parsed, Capability::ScreenCapture { .. });
        let lifetime = if one_time {
            SENSITIVE_TOKEN_LIFETIME
        } else {
            DEFAULT_TOKEN_LIFETIME
        };
        let expiry =
            unix_ms().saturating_add(u64::try_from(lifetime.as_millis()).unwrap_or(u64::MAX));
        let mut nonce = [0_u8; TOKEN_NONCE_BYTES];
        self.random
            .fill(&mut nonce)
            .map_err(|_| io::Error::other("kernel randomness unavailable"))?;
        let app_len = u16::try_from(app_id.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "application ID too long"))?;
        let cap_len = u16::try_from(capability.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "capability name too long"))?;
        let mut token = Vec::with_capacity(
            TOKEN_FIXED_BYTES + app_id.len() + capability.len() + TOKEN_TAG_BYTES,
        );
        token.extend_from_slice(TOKEN_MAGIC);
        token.push(TOKEN_VERSION);
        token.push(u8::from(one_time));
        token.extend_from_slice(&expiry.to_be_bytes());
        token.extend_from_slice(&generation.to_be_bytes());
        token.extend_from_slice(&app_len.to_be_bytes());
        token.extend_from_slice(&cap_len.to_be_bytes());
        token.extend_from_slice(&nonce);
        token.extend_from_slice(app_id.as_bytes());
        token.extend_from_slice(capability.as_bytes());
        let tag = hmac::sign(&self.signing_key, &token);
        token.extend_from_slice(tag.as_ref());
        Ok((token, expiry, one_time))
    }

    fn verify_signed_token(&self, token: &[u8]) -> Option<TokenClaims> {
        if token.len() < TOKEN_FIXED_BYTES + TOKEN_TAG_BYTES
            || token.get(..4)? != TOKEN_MAGIC
            || token[4] != TOKEN_VERSION
            || token[5] > 1
        {
            return None;
        }
        let signed_len = token.len().checked_sub(TOKEN_TAG_BYTES)?;
        hmac::verify(
            &self.signing_key,
            &token[..signed_len],
            &token[signed_len..],
        )
        .ok()?;
        let expiry = u64::from_be_bytes(token.get(6..14)?.try_into().ok()?);
        let generation = u64::from_be_bytes(token.get(14..22)?.try_into().ok()?);
        let app_len = usize::from(u16::from_be_bytes(token.get(22..24)?.try_into().ok()?));
        let cap_len = usize::from(u16::from_be_bytes(token.get(24..26)?.try_into().ok()?));
        let fields_start = TOKEN_FIXED_BYTES;
        let app_end = fields_start.checked_add(app_len)?;
        let cap_end = app_end.checked_add(cap_len)?;
        if cap_end != signed_len || expiry <= unix_ms() {
            return None;
        }
        let app_id = std::str::from_utf8(token.get(fields_start..app_end)?)
            .ok()?
            .to_owned();
        let capability = std::str::from_utf8(token.get(app_end..cap_end)?)
            .ok()?
            .to_owned();
        if !valid_app_id(&app_id) || Capability::from_wire_name(&capability).is_none() {
            return None;
        }
        let ledger = self.read_grants().ok()?;
        if generation != ledger.generation
            || Self::policy(&ledger, &app_id, &capability) != PolicyDecision::Allow
        {
            return None;
        }
        let tag: [u8; TOKEN_TAG_BYTES] = token.get(signed_len..)?.try_into().ok()?;
        let one_time = token[5] == 1;
        if one_time && !self.consume_once(tag, expiry).ok()? {
            return None;
        }
        Some(TokenClaims { app_id, capability })
    }

    /// Persist consumption before allowing a single-use operation. A daemon
    /// restart therefore cannot make an already-used token valid again.
    #[allow(clippy::significant_drop_tightening)] // lock serializes check + durable append
    fn consume_once(&self, tag: [u8; TOKEN_TAG_BYTES], expiry: u64) -> io::Result<bool> {
        let mut consumed = self
            .consumed_tokens
            .lock()
            .map_err(|_| io::Error::other("consumed-token lock poisoned"))?;
        let now = unix_ms();
        consumed.retain(|_, expires| *expires > now);
        if consumed.contains_key(&tag) {
            return Ok(false);
        }
        consumed.insert(tag, expiry);
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(self.config.state_dir.join("consumed.tsv"))?;
        writeln!(file, "consumed\t{expiry}\t{}", encode_hex(&tag))?;
        file.sync_data()?;
        Ok(true)
    }

    fn policy(ledger: &GrantLedger, app_id: &str, capability_name: &str) -> PolicyDecision {
        if let Some(decision) = ledger
            .decisions
            .get(&(app_id.to_owned(), capability_name.to_owned()))
        {
            return *decision;
        }
        let Some(capability) = Capability::from_wire_name(capability_name) else {
            return PolicyDecision::Deny;
        };
        if capability::default_app_capabilities().contains(&capability) {
            return PolicyDecision::Allow;
        }
        if capability::shell_only_capabilities().contains(&capability) {
            return if app_id == SHELL_APP_ID {
                PolicyDecision::Allow
            } else {
                PolicyDecision::Deny
            };
        }
        if capability::lock_only_capabilities().contains(&capability) {
            return if app_id == LOCK_SERVICE_APP_ID {
                PolicyDecision::Allow
            } else {
                PolicyDecision::Deny
            };
        }
        PolicyDecision::Prompt
    }

    fn read_grants(&self) -> io::Result<GrantLedger> {
        let file = match secure_read_file(&self.config.grants) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(GrantLedger {
                    generation: 1,
                    decisions: HashMap::new(),
                });
            }
            Err(error) => return Err(error),
        };
        let mut ledger = GrantLedger::default();
        let mut saw_version = false;
        for line in BufReader::new(file).lines() {
            let line = line?;
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line == "version\t1" {
                saw_version = true;
                continue;
            }
            if let Some(value) = line.strip_prefix("generation\t") {
                ledger.generation = value
                    .parse()
                    .map_err(|_| invalid_data("invalid grant generation"))?;
                continue;
            }
            let fields: Vec<_> = line.split('\t').collect();
            if fields.len() != 4
                || fields[0] != "grant"
                || !valid_app_id(fields[1])
                || Capability::from_wire_name(fields[2]).is_none()
            {
                return Err(invalid_data("invalid grant record"));
            }
            let decision = match fields[3] {
                "allow" => PolicyDecision::Allow,
                "deny" => PolicyDecision::Deny,
                "prompt" => PolicyDecision::Prompt,
                _ => return Err(invalid_data("invalid grant decision")),
            };
            if ledger
                .decisions
                .insert((fields[1].to_owned(), fields[2].to_owned()), decision)
                .is_some()
            {
                return Err(invalid_data("duplicate grant record"));
            }
        }
        if !saw_version || ledger.generation == 0 {
            return Err(invalid_data(
                "grant ledger requires version 1 and non-zero generation",
            ));
        }
        Ok(ledger)
    }

    fn verify_identity(&self, pid: u32) -> Option<String> {
        let proc_dir = PathBuf::from(format!("/proc/{pid}"));
        let executable = fs::read_link(proc_dir.join("exe"))
            .ok()?
            .canonicalize()
            .ok()?;
        let uid = process_uid(&proc_dir.join("status"))?;

        if executable_matches(&executable, &self.config.trusted_bin_dir, SHELL_APP_ID) {
            return Some(SHELL_APP_ID.to_owned());
        }
        if executable_matches(
            &executable,
            &self.config.trusted_bin_dir,
            LOCK_SERVICE_APP_ID,
        ) {
            return Some(LOCK_SERVICE_APP_ID.to_owned());
        }

        let registry = secure_read_file(&self.config.identity_registry).ok()?;
        let mut saw_version = false;
        let mut matched = None;
        for line in BufReader::new(registry).lines() {
            let line = line.ok()?;
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line == "version\t1" {
                saw_version = true;
                continue;
            }
            let fields: Vec<_> = line.split('\t').collect();
            if fields.len() != 4 || fields[0] != "identity" || !valid_app_id(fields[1]) {
                return None;
            }
            let record_uid: u32 = fields[2].parse().ok()?;
            let registered = Path::new(fields[3]).canonicalize().ok()?;
            if record_uid == uid
                && registered == executable
                && matched.replace(fields[1].to_owned()).is_some()
            {
                return None;
            }
        }
        saw_version.then_some(matched).flatten()
    }

    fn append_audit(
        &self,
        app_id: &str,
        capability: &str,
        outcome: &str,
        source: &str,
    ) -> io::Result<()> {
        if !valid_app_id(app_id)
            || Capability::from_wire_name(capability).is_none()
            || !outcome
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'-')
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid audit fields",
            ));
        }
        let _guard = self
            .audit_lock
            .lock()
            .map_err(|_| io::Error::other("audit lock poisoned"))?;
        let audit_path = self.config.audit_path();
        if fs::metadata(&audit_path).is_ok_and(|metadata| metadata.len() >= MAX_AUDIT_BYTES) {
            fs::rename(&audit_path, audit_path.with_extension("tsv.previous"))?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(audit_path)?;
        writeln!(
            file,
            "audit\t{}\t{}\t{}\t{}\t{}",
            unix_ms(),
            app_id,
            capability,
            outcome,
            source
        )?;
        file.sync_data()
    }
}

/// Bind the private daemon socket and serve until `shutdown` is set.
///
/// # Errors
///
/// Returns an error when the private socket cannot be bound, polled, or
/// removed cleanly after shutdown.
pub fn serve(service: &Arc<SecurityService>, shutdown: &AtomicBool) -> io::Result<()> {
    let listener = bind_private_listener(&service.config.socket_path)?;
    listener.set_nonblocking(true)?;
    while !shutdown.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                let service = Arc::clone(service);
                thread::spawn(move || {
                    if let Err(error) = serve_connection(&service, stream) {
                        tracing::warn!(%error, "security IPC request failed");
                    }
                });
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => thread::sleep(ACCEPT_POLL),
            Err(error) => return Err(error),
        }
    }
    drop(listener);
    match fs::remove_file(&service.config.socket_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn serve_connection(service: &SecurityService, mut stream: UnixStream) -> io::Result<()> {
    let credentials = getsockopt(&stream, PeerCredentials).map_err(io::Error::other)?;
    if credentials.uid() != Uid::effective().as_raw() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "security IPC peer UID rejected",
        ));
    }
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let request: SecurityRequest = read_security_frame(&mut stream)?;
    write_security_frame(&mut stream, &service.handle(request))
}

fn bind_private_listener(path: &Path) -> io::Result<UnixListener> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "socket has no parent"))?;
    fs::create_dir_all(parent)?;
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if !metadata.file_type().is_socket() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "refusing to replace non-socket path",
            ));
        }
        match UnixStream::connect(path) {
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    "sol-securityd already running",
                ));
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
                ) =>
            {
                fs::remove_file(path)?;
            }
            Err(error) => return Err(error),
        }
    }
    let listener = UnixListener::bind(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

fn load_or_create_key(path: &Path) -> io::Result<[u8; 32]> {
    match secure_read_file(path) {
        Ok(mut file) => {
            let mut key = [0_u8; 32];
            file.read_exact(&mut key)?;
            let mut extra = [0_u8; 1];
            if file.read(&mut extra)? != 0 {
                return Err(invalid_data("token key has wrong size"));
            }
            Ok(key)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let mut key = [0_u8; 32];
            SystemRandom::new()
                .fill(&mut key)
                .map_err(|_| io::Error::other("kernel randomness unavailable"))?;
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)
            {
                Ok(mut file) => {
                    file.write_all(&key)?;
                    file.sync_all()?;
                    Ok(key)
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    load_or_create_key(path)
                }
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

fn load_consumed_tokens(path: &Path) -> io::Result<HashMap<[u8; TOKEN_TAG_BYTES], u64>> {
    let file = match secure_read_file(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(error) => return Err(error),
    };
    let now = unix_ms();
    let mut consumed = HashMap::new();
    for line in BufReader::new(file).lines() {
        let line = line?;
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 3 || fields[0] != "consumed" {
            return Err(invalid_data("invalid consumed-token record"));
        }
        let expiry: u64 = fields[1]
            .parse()
            .map_err(|_| invalid_data("invalid consumed-token expiry"))?;
        let tag = decode_tag(fields[2])?;
        if expiry > now {
            consumed.insert(tag, expiry);
        }
    }
    Ok(consumed)
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn decode_tag(value: &str) -> io::Result<[u8; TOKEN_TAG_BYTES]> {
    if value.len() != TOKEN_TAG_BYTES * 2 {
        return Err(invalid_data("invalid consumed-token tag length"));
    }
    let mut tag = [0_u8; TOKEN_TAG_BYTES];
    for (output, pair) in tag.iter_mut().zip(value.as_bytes().as_chunks::<2>().0) {
        *output = (hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?;
    }
    Ok(tag)
}

fn hex_nibble(byte: u8) -> io::Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid consumed-token tag",
        )),
    }
}

fn secure_read_file(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_CLOEXEC)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.mode() & 0o022 != 0
        || metadata.uid() != Uid::effective().as_raw()
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "{} is not a private daemon-owned regular file",
                path.display()
            ),
        ));
    }
    Ok(file)
}

fn executable_matches(executable: &Path, trusted_dir: &Path, name: &str) -> bool {
    let Ok(trusted) = trusted_dir.canonicalize() else {
        return false;
    };
    executable.parent() == Some(trusted.as_path()) && executable.file_name() == Some(name.as_ref())
}

fn process_uid(status_path: &Path) -> Option<u32> {
    fs::read_to_string(status_path)
        .ok()?
        .lines()
        .find_map(|line| {
            line.strip_prefix("Uid:")?
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        })
}

fn valid_app_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && !value.starts_with('.')
        && !value.ends_with('.')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn error_response(context: &str, error: &io::Error) -> SecurityResponse {
    SecurityResponse::Error {
        message: format!("{context}: {error}"),
    }
}

const fn audit_name(outcome: AuditOutcome) -> &'static str {
    match outcome {
        AuditOutcome::Granted => "granted",
        AuditOutcome::Denied => "denied",
        AuditOutcome::Used => "used",
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use super::*;
    use sol_compositor::scp::security::{AppId, DaemonSecurityCoordinator, SecurityCoordinator};
    use tempfile::TempDir;

    fn fixture() -> (TempDir, SecurityConfig) {
        let temp = tempfile::tempdir().expect("temporary directory");
        let state = temp.path().join("state");
        fs::create_dir(&state).expect("state directory");
        let config = SecurityConfig {
            socket_path: temp.path().join("securityd.sock"),
            state_dir: state.clone(),
            identity_registry: state.join("identities.tsv"),
            grants: state.join("grants.tsv"),
            trusted_bin_dir: temp.path().join("trusted"),
        };
        (temp, config)
    }

    #[test]
    fn signed_token_rejects_tampering_and_replay() {
        let (_temp, config) = fixture();
        fs::write(
            &config.grants,
            "version\t1\ngeneration\t1\ngrant\torg.sol.test\tscreen-capture-output\tallow\n",
        )
        .expect("grants");
        fs::set_permissions(&config.grants, fs::Permissions::from_mode(0o600)).expect("mode");
        let service = SecurityService::open(config).expect("service");
        let (mut token, _, _) = service
            .issue("org.sol.test", "screen-capture-output", 1)
            .expect("token");
        assert!(service.verify_signed_token(&token).is_some());
        assert!(
            service.verify_signed_token(&token).is_none(),
            "one-time token must not replay"
        );
        let last = token.len() - 1;
        token[last] ^= 1;
        assert!(service.verify_signed_token(&token).is_none());
    }

    #[test]
    fn one_time_consumption_survives_daemon_restart() {
        let (_temp, config) = fixture();
        fs::write(
            &config.grants,
            "version\t1\ngeneration\t1\ngrant\torg.sol.test\tscreen-capture-output\tallow\n",
        )
        .expect("grants");
        fs::set_permissions(&config.grants, fs::Permissions::from_mode(0o600)).expect("mode");
        let token = {
            let service = SecurityService::open(config.clone()).expect("first service");
            let (token, _, _) = service
                .issue("org.sol.test", "screen-capture-output", 1)
                .expect("token");
            assert!(service.verify_signed_token(&token).is_some());
            token
        };
        let restarted = SecurityService::open(config).expect("restarted service");
        assert!(
            restarted.verify_signed_token(&token).is_none(),
            "durably consumed token must remain invalid after restart"
        );
    }

    #[test]
    fn ledger_generation_revokes_outstanding_tokens() {
        let (_temp, config) = fixture();
        fs::write(
            &config.grants,
            "version\t1\ngeneration\t4\ngrant\torg.sol.test\tfullscreen\tallow\n",
        )
        .expect("grants");
        fs::set_permissions(&config.grants, fs::Permissions::from_mode(0o600)).expect("mode");
        let service = SecurityService::open(config.clone()).expect("service");
        let (token, _, _) = service
            .issue("org.sol.test", "fullscreen", 4)
            .expect("token");
        assert!(service.verify_signed_token(&token).is_some());
        fs::write(
            &config.grants,
            "version\t1\ngeneration\t5\ngrant\torg.sol.test\tfullscreen\tdeny\n",
        )
        .expect("revoke");
        assert!(service.verify_signed_token(&token).is_none());
    }

    #[test]
    fn daemon_coordinator_round_trips_real_ipc() {
        let (_temp, config) = fixture();
        let service = Arc::new(SecurityService::open(config.clone()).expect("service"));
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = Arc::clone(&shutdown);
        let worker = thread::spawn(move || serve(&service, &thread_shutdown).expect("serve"));
        for _ in 0..100 {
            if config.socket_path.exists() {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        let coordinator = DaemonSecurityCoordinator::new(config.socket_path);
        let app = AppId("org.sol.test".to_owned());
        let decision = coordinator.evaluate_capability(&app, &Capability::WindowToplevel);
        let sol_compositor::scp::capability::Decision::Granted { token, .. } = decision else {
            panic!("default capability must be granted");
        };
        assert_eq!(
            coordinator.verify_token(&token),
            Some((app, Capability::WindowToplevel))
        );
        shutdown.store(true, Ordering::Release);
        worker.join().expect("worker");
    }

    #[test]
    fn insecure_policy_file_fails_closed() {
        let (_temp, config) = fixture();
        fs::write(
            &config.grants,
            "version\t1\ngeneration\t1\ngrant\torg.sol.test\tfullscreen\tallow\n",
        )
        .expect("grants");
        fs::set_permissions(&config.grants, fs::Permissions::from_mode(0o666)).expect("mode");
        let service = SecurityService::open(config).expect("service");
        assert!(matches!(
            service.evaluate("org.sol.test", "fullscreen"),
            WireDecision::Denied { .. }
        ));
    }

    #[test]
    fn registry_binds_exact_executable_and_uid_to_app_id() {
        let (_temp, config) = fixture();
        let executable = fs::read_link(format!("/proc/{}/exe", std::process::id()))
            .expect("own executable")
            .canonicalize()
            .expect("canonical executable");
        fs::write(
            &config.identity_registry,
            format!(
                "version\t1\nidentity\torg.sol.security-test\t{}\t{}\n",
                Uid::effective().as_raw(),
                executable.display()
            ),
        )
        .expect("identity registry");
        fs::set_permissions(&config.identity_registry, fs::Permissions::from_mode(0o600))
            .expect("registry mode");
        let service = SecurityService::open(config).expect("service");
        assert_eq!(
            service.verify_identity(std::process::id()),
            Some("org.sol.security-test".to_owned())
        );
    }

    #[test]
    fn authorization_is_audited_to_a_private_file() {
        let (_temp, config) = fixture();
        let service = SecurityService::open(config.clone()).expect("service");
        assert!(matches!(
            service.evaluate("org.sol.test", "window-toplevel"),
            WireDecision::Granted { .. }
        ));
        let metadata = fs::metadata(config.audit_path()).expect("audit metadata");
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        let audit = fs::read_to_string(config.audit_path()).expect("audit contents");
        assert!(audit.contains("\torg.sol.test\twindow-toplevel\tgranted\tevaluate\n"));
    }
}
