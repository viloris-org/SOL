//! Privacy-bounded diagnostics and crash-reporting foundation.
//!
//! This crate records a deliberately small, typed event schema. It has no API
//! for commands, environment variables, process arguments, stack traces, or
//! opaque attachments. Future collection or upload transports must consume
//! [`DiagnosticRecord`] values and establish their own explicit consent and
//! retention policy.

use sol_app::AppId;
use std::error::Error;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::panic::{self, PanicHookInfo};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const STORAGE_VERSION: u32 = 1;
const MAX_MESSAGE_CHARS: usize = 240;
static PANIC_CAPTURE_INSTALLED: OnceLock<()> = OnceLock::new();

/// Result type returned by the diagnostics service and stores.
pub type DiagnosticResult<T> = Result<T, DiagnosticError>;

/// A diagnostics service or storage failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticError {
    /// A configured retention limit is not usable.
    InvalidRetentionLimit,
    /// The clock could not provide a Unix timestamp.
    Clock(String),
    /// Persistent storage could not be read or written safely.
    Storage(String),
    /// A process may install only one diagnostics panic hook.
    PanicCaptureAlreadyInstalled,
}

impl fmt::Display for DiagnosticError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRetentionLimit => {
                formatter.write_str("diagnostic retention limit must be positive")
            }
            Self::Clock(error) => write!(formatter, "diagnostic clock failure: {error}"),
            Self::Storage(error) => write!(formatter, "diagnostic storage failure: {error}"),
            Self::PanicCaptureAlreadyInstalled => {
                formatter.write_str("diagnostic panic capture is already installed")
            }
        }
    }
}

impl Error for DiagnosticError {}

/// Known SOL runtime components allowed to emit diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolComponent {
    /// The native SCP compositor.
    Compositor,
    /// The desktop shell.
    Shell,
    /// The settings daemon.
    SettingsDaemon,
    /// The notification daemon.
    NotificationDaemon,
    /// The desktop portal service.
    Portal,
    /// The input-method frontend.
    InputMethod,
    /// The diagnostics service itself.
    Diagnostics,
}

impl SolComponent {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Compositor => "compositor",
            Self::Shell => "shell",
            Self::SettingsDaemon => "settingsd",
            Self::NotificationDaemon => "notificationd",
            Self::Portal => "portal",
            Self::InputMethod => "ime",
            Self::Diagnostics => "diagnostics",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "compositor" => Some(Self::Compositor),
            "shell" => Some(Self::Shell),
            "settingsd" => Some(Self::SettingsDaemon),
            "notificationd" => Some(Self::NotificationDaemon),
            "portal" => Some(Self::Portal),
            "ime" => Some(Self::InputMethod),
            "diagnostics" => Some(Self::Diagnostics),
            _ => None,
        }
    }
}

/// The typed origin declared by a diagnostic event producer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticSource {
    /// A first-party runtime component.
    Component(SolComponent),
    /// A SOL application identified by its validated application ID.
    Application(AppId),
}

impl DiagnosticSource {
    fn encode(&self) -> String {
        match self {
            Self::Component(component) => format!("component:{}", component.as_str()),
            Self::Application(app_id) => format!("application:{app_id}"),
        }
    }

    fn parse(value: &str) -> DiagnosticResult<Self> {
        let (kind, value) = value
            .split_once(':')
            .ok_or_else(|| DiagnosticError::Storage("invalid diagnostic source".to_owned()))?;
        match kind {
            "component" => SolComponent::parse(value)
                .map(Self::Component)
                .ok_or_else(|| DiagnosticError::Storage("unknown diagnostic component".to_owned())),
            "application" => AppId::parse(value).map(Self::Application).map_err(|_| {
                DiagnosticError::Storage("invalid diagnostic application ID".to_owned())
            }),
            _ => Err(DiagnosticError::Storage(
                "unknown diagnostic source kind".to_owned(),
            )),
        }
    }
}

/// Severity used for local diagnosis and future consent UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    /// The component is degraded but remains available.
    Warning,
    /// The requested operation failed.
    Error,
    /// The component crashed or cannot continue.
    Fatal,
}

impl DiagnosticSeverity {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Fatal => "fatal",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "warning" => Some(Self::Warning),
            "error" => Some(Self::Error),
            "fatal" => Some(Self::Fatal),
            _ => None,
        }
    }
}

/// Allowlisted failure categories. Free-form event names are intentionally absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticCode {
    /// A component process terminated unexpectedly.
    ProcessCrash,
    /// Startup or initialization failed.
    InitializationFailure,
    /// The bounded diagnostics store could not be used.
    StorageFailure,
    /// An expected service transport failed.
    TransportFailure,
    /// A typed protocol contract was violated.
    ProtocolViolation,
    /// An authorized boundary denied an operation.
    PermissionDenied,
    /// A graphics, input, or display device failed.
    HardwareFailure,
}

impl DiagnosticCode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::ProcessCrash => "process-crash",
            Self::InitializationFailure => "initialization-failure",
            Self::StorageFailure => "storage-failure",
            Self::TransportFailure => "transport-failure",
            Self::ProtocolViolation => "protocol-violation",
            Self::PermissionDenied => "permission-denied",
            Self::HardwareFailure => "hardware-failure",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "process-crash" => Some(Self::ProcessCrash),
            "initialization-failure" => Some(Self::InitializationFailure),
            "storage-failure" => Some(Self::StorageFailure),
            "transport-failure" => Some(Self::TransportFailure),
            "protocol-violation" => Some(Self::ProtocolViolation),
            "permission-denied" => Some(Self::PermissionDenied),
            "hardware-failure" => Some(Self::HardwareFailure),
            _ => None,
        }
    }
}

/// Bounded text retained after deterministic secret and home-path redaction.
///
/// The constructor is the sole public way to create this type. It strips
/// control characters, redacts common credential forms and home paths, then
/// truncates the result. This is defensive filtering, not a substitute for a
/// future consent policy: callers should provide a short operational summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactedDiagnosticText(String);

impl RedactedDiagnosticText {
    /// Redact and bound a short diagnostic summary.
    #[must_use]
    pub fn new(value: impl AsRef<str>) -> Self {
        let normalized: String = value
            .as_ref()
            .chars()
            .map(|character| {
                if character.is_control() {
                    ' '
                } else {
                    character
                }
            })
            .collect();
        let redacted = redact_home_paths(&redact_credentials(&normalized));
        Self(redacted.trim().chars().take(MAX_MESSAGE_CHARS).collect())
    }

    /// Return the redacted, bounded summary.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A typed event with no facility for arbitrary process or payload capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticEvent {
    /// The typed event origin declared by the producer.
    pub source: DiagnosticSource,
    /// The event severity.
    pub severity: DiagnosticSeverity,
    /// The allowlisted event category.
    pub code: DiagnosticCode,
    message: Option<RedactedDiagnosticText>,
}

impl DiagnosticEvent {
    /// Construct an event without a free-form payload.
    #[must_use]
    pub const fn new(
        source: DiagnosticSource,
        severity: DiagnosticSeverity,
        code: DiagnosticCode,
    ) -> Self {
        Self {
            source,
            severity,
            code,
            message: None,
        }
    }

    /// Add a redacted, bounded operational summary.
    #[must_use]
    pub fn with_message(mut self, message: impl AsRef<str>) -> Self {
        self.message = Some(RedactedDiagnosticText::new(message));
        self
    }

    /// Return the retained operational summary, if a producer supplied one.
    #[must_use]
    pub fn message(&self) -> Option<&RedactedDiagnosticText> {
        self.message.as_ref()
    }
}

/// A stored, sequence-addressable diagnostic event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticRecord {
    /// Monotonic sequence number assigned by [`DiagnosticsService`].
    pub sequence: u64,
    /// Milliseconds since the Unix epoch when the service accepted the event.
    pub occurred_at_unix_ms: u64,
    /// The typed event content.
    pub event: DiagnosticEvent,
}

/// The complete daemon-private state retained by a diagnostics store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticSnapshot {
    /// Sequence number to assign to the next event.
    pub next_sequence: u64,
    /// Oldest-to-newest retained records.
    pub records: Vec<DiagnosticRecord>,
}

impl Default for DiagnosticSnapshot {
    fn default() -> Self {
        Self {
            next_sequence: 1,
            records: Vec::new(),
        }
    }
}

/// Explicit maximum number of retained records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagnosticRetention {
    maximum_records: usize,
}

impl DiagnosticRetention {
    /// Create a retention policy. Zero records is rejected to avoid a silent no-op service.
    pub fn new(maximum_records: usize) -> DiagnosticResult<Self> {
        if maximum_records == 0 {
            return Err(DiagnosticError::InvalidRetentionLimit);
        }
        Ok(Self { maximum_records })
    }

    /// Return the maximum retained records.
    #[must_use]
    pub const fn maximum_records(self) -> usize {
        self.maximum_records
    }
}

/// The default local retention ceiling.
pub const DEFAULT_RETENTION: DiagnosticRetention = DiagnosticRetention {
    maximum_records: 256,
};

/// Return the daemon-private default diagnostics path for the current user.
#[must_use]
pub fn default_diagnostics_path() -> PathBuf {
    if let Some(state_home) = std::env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(state_home).join("sol/diagnostics.log");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".local/state/sol/diagnostics.log");
    }
    PathBuf::from("sol/diagnostics.log")
}

/// Install a process-global panic hook that records one typed, redacted crash
/// event before delegating to Rust's existing hook.
///
/// This must be called once during process startup. It captures no backtrace,
/// arguments, environment, attachment, or opaque crash dump.
pub fn install_panic_capture(
    source: DiagnosticSource,
    path: impl Into<PathBuf>,
    retention: DiagnosticRetention,
) -> DiagnosticResult<()> {
    let service = DiagnosticsService::new(FileDiagnosticStore::new(path), retention)?;
    PANIC_CAPTURE_INSTALLED
        .set(())
        .map_err(|()| DiagnosticError::PanicCaptureAlreadyInstalled)?;
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |information| {
        let event = DiagnosticEvent::new(
            source.clone(),
            DiagnosticSeverity::Fatal,
            DiagnosticCode::ProcessCrash,
        )
        .with_message(panic_summary(information));
        let _ = service.record(event);
        previous(information);
    }));
    Ok(())
}

/// Install panic capture using [`default_diagnostics_path`] and
/// [`DEFAULT_RETENTION`].
pub fn install_default_panic_capture(source: DiagnosticSource) -> DiagnosticResult<()> {
    install_panic_capture(source, default_diagnostics_path(), DEFAULT_RETENTION)
}

/// Storage boundary for bounded diagnostics history.
pub trait DiagnosticStore: Send + Sync {
    /// Load the most recently persisted daemon state, if it exists.
    fn load(&self) -> DiagnosticResult<Option<DiagnosticSnapshot>>;

    /// Persist a complete, already bounded daemon state.
    fn save(&self, snapshot: &DiagnosticSnapshot) -> DiagnosticResult<()>;
}

/// In-memory diagnostics storage for tests and embedded development.
#[derive(Debug, Default)]
pub struct MemoryDiagnosticStore {
    snapshot: Mutex<Option<DiagnosticSnapshot>>,
}

impl MemoryDiagnosticStore {
    /// Construct an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl DiagnosticStore for MemoryDiagnosticStore {
    fn load(&self) -> DiagnosticResult<Option<DiagnosticSnapshot>> {
        self.snapshot
            .lock()
            .map_err(|error| {
                DiagnosticError::Storage(format!("diagnostic store lock poisoned: {error}"))
            })
            .map(|snapshot| snapshot.clone())
    }

    fn save(&self, snapshot: &DiagnosticSnapshot) -> DiagnosticResult<()> {
        let mut stored = self.snapshot.lock().map_err(|error| {
            DiagnosticError::Storage(format!("diagnostic store lock poisoned: {error}"))
        })?;
        *stored = Some(snapshot.clone());
        Ok(())
    }
}

/// A line-oriented, atomically replaced, daemon-private diagnostics file.
#[derive(Debug, Clone)]
pub struct FileDiagnosticStore {
    path: PathBuf,
}

impl FileDiagnosticStore {
    /// Create a store backed by `path`.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Return the backing file path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl DiagnosticStore for FileDiagnosticStore {
    fn load(&self) -> DiagnosticResult<Option<DiagnosticSnapshot>> {
        let contents = match fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error("read diagnostics", error)),
        };
        parse_snapshot(&contents).map(Some)
    }

    fn save(&self, snapshot: &DiagnosticSnapshot) -> DiagnosticResult<()> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)
            .map_err(|error| io_error("create diagnostics directory", error))?;

        let temporary_path = temporary_path(&self.path)?;
        let write_result = write_snapshot(&temporary_path, snapshot);
        if let Err(error) = write_result {
            let _ = fs::remove_file(&temporary_path);
            return Err(error);
        }
        fs::rename(&temporary_path, &self.path)
            .map_err(|error| io_error("replace diagnostics", error))?;
        restrict_file_permissions(&self.path)
            .map_err(|error| io_error("restrict diagnostics permissions", error))?;
        sync_directory(parent).map_err(|error| io_error("sync diagnostics directory", error))
    }
}

/// Typed service that timestamps, sequences, bounds, and persists diagnostics.
#[derive(Debug)]
pub struct DiagnosticsService<S> {
    store: S,
    retention: DiagnosticRetention,
    snapshot: Mutex<DiagnosticSnapshot>,
}

impl<S: DiagnosticStore> DiagnosticsService<S> {
    /// Restore the bounded history from `store` or initialize an empty history.
    pub fn new(store: S, retention: DiagnosticRetention) -> DiagnosticResult<Self> {
        let mut snapshot = store.load()?.unwrap_or_default();
        validate_snapshot(&snapshot)?;
        let was_trimmed = trim_snapshot(&mut snapshot, retention);
        if was_trimmed {
            store.save(&snapshot)?;
        }
        Ok(Self {
            store,
            retention,
            snapshot: Mutex::new(snapshot),
        })
    }

    /// Return the backing store for service setup and diagnostics tests.
    #[must_use]
    pub fn store(&self) -> &S {
        &self.store
    }

    /// Record a typed event after applying retention and write-through persistence.
    pub fn record(&self, event: DiagnosticEvent) -> DiagnosticResult<DiagnosticRecord> {
        let occurred_at_unix_ms = unix_time_ms()?;
        let mut current = self.snapshot.lock().map_err(|error| {
            DiagnosticError::Storage(format!("diagnostic state lock poisoned: {error}"))
        })?;
        let sequence = current
            .next_sequence
            .checked_add(1)
            .ok_or_else(|| DiagnosticError::Storage("diagnostic sequence overflow".to_owned()))?;
        let record = DiagnosticRecord {
            sequence: current.next_sequence,
            occurred_at_unix_ms,
            event,
        };
        let mut next = current.clone();
        next.next_sequence = sequence;
        next.records.push(record.clone());
        trim_snapshot(&mut next, self.retention);
        self.store.save(&next)?;
        *current = next;
        Ok(record)
    }

    /// Return retained records from oldest to newest.
    pub fn records(&self) -> DiagnosticResult<Vec<DiagnosticRecord>> {
        self.snapshot
            .lock()
            .map_err(|error| {
                DiagnosticError::Storage(format!("diagnostic state lock poisoned: {error}"))
            })
            .map(|snapshot| snapshot.records.clone())
    }
}

fn trim_snapshot(snapshot: &mut DiagnosticSnapshot, retention: DiagnosticRetention) -> bool {
    let excess = snapshot
        .records
        .len()
        .saturating_sub(retention.maximum_records());
    if excess == 0 {
        return false;
    }
    snapshot.records.drain(..excess);
    true
}

fn unix_time_ms() -> DiagnosticResult<u64> {
    let milliseconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| DiagnosticError::Clock(error.to_string()))?
        .as_millis();
    u64::try_from(milliseconds)
        .map_err(|_| DiagnosticError::Clock("Unix timestamp exceeds u64 milliseconds".to_owned()))
}

fn panic_summary(information: &PanicHookInfo<'_>) -> String {
    let payload = information
        .payload()
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| {
            information
                .payload()
                .downcast_ref::<String>()
                .map(String::as_str)
        })
        .unwrap_or("non-string panic payload");
    information.location().map_or_else(
        || format!("panic: {payload}"),
        |location| {
            format!(
                "panic at {}:{}:{}: {payload}",
                location.file(),
                location.line(),
                location.column()
            )
        },
    )
}

fn temporary_path(path: &Path) -> DiagnosticResult<PathBuf> {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            DiagnosticError::Storage("diagnostics path must have a UTF-8 file name".to_owned())
        })?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| DiagnosticError::Clock(error.to_string()))?
        .as_nanos();
    Ok(path.with_file_name(format!(".{filename}.tmp-{}-{nonce}", std::process::id())))
}

fn write_snapshot(path: &Path, snapshot: &DiagnosticSnapshot) -> DiagnosticResult<()> {
    let mut file = create_private_file(path)
        .map_err(|error| io_error("create temporary diagnostics", error))?;
    file.write_all(serialize_snapshot(snapshot).as_bytes())
        .map_err(|error| io_error("write temporary diagnostics", error))?;
    file.sync_all()
        .map_err(|error| io_error("sync temporary diagnostics", error))
}

fn create_private_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn restrict_file_permissions(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

fn sync_directory(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(path)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

fn serialize_snapshot(snapshot: &DiagnosticSnapshot) -> String {
    let mut output = format!(
        "# SOL diagnostics storage; format version {STORAGE_VERSION}\nversion={STORAGE_VERSION}\nnext_sequence={}\n",
        snapshot.next_sequence
    );
    for record in &snapshot.records {
        let message = record.event.message().map_or_else(String::new, |message| {
            encode_hex(message.as_str().as_bytes())
        });
        output.push_str(&format!(
            "record={}\t{}\t{}\t{}\t{}\t{message}\n",
            record.sequence,
            record.occurred_at_unix_ms,
            record.event.source.encode(),
            record.event.severity.as_str(),
            record.event.code.as_str(),
        ));
    }
    output
}

fn parse_snapshot(contents: &str) -> DiagnosticResult<DiagnosticSnapshot> {
    let mut version = None;
    let mut next_sequence = None;
    let mut records = Vec::new();
    for line in contents.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(value) = line.strip_prefix("version=") {
            version = Some(parse_number(value, "storage version")?);
        } else if let Some(value) = line.strip_prefix("next_sequence=") {
            next_sequence = Some(parse_number(value, "next sequence")?);
        } else if let Some(value) = line.strip_prefix("record=") {
            records.push(parse_record(value)?);
        }
    }
    match version {
        Some(STORAGE_VERSION) => {}
        Some(version) => {
            return Err(DiagnosticError::Storage(format!(
                "unsupported diagnostics storage version {version}"
            )));
        }
        None => {
            return Err(DiagnosticError::Storage(
                "diagnostics storage has no version".to_owned(),
            ));
        }
    }
    let snapshot = DiagnosticSnapshot {
        next_sequence: next_sequence.ok_or_else(|| {
            DiagnosticError::Storage("diagnostics storage has no next sequence".to_owned())
        })?,
        records,
    };
    validate_snapshot(&snapshot)?;
    Ok(snapshot)
}

fn parse_record(value: &str) -> DiagnosticResult<DiagnosticRecord> {
    let fields: Vec<_> = value.split('\t').collect();
    if fields.len() != 6 {
        return Err(DiagnosticError::Storage(
            "invalid diagnostic record field count".to_owned(),
        ));
    }
    let message = if fields[5].is_empty() {
        None
    } else {
        Some(decode_hex(fields[5])?)
    };
    let mut event = DiagnosticEvent::new(
        DiagnosticSource::parse(fields[2])?,
        DiagnosticSeverity::parse(fields[3])
            .ok_or_else(|| DiagnosticError::Storage("invalid diagnostic severity".to_owned()))?,
        DiagnosticCode::parse(fields[4])
            .ok_or_else(|| DiagnosticError::Storage("invalid diagnostic code".to_owned()))?,
    );
    if let Some(message) = message {
        event = event.with_message(message);
    }
    Ok(DiagnosticRecord {
        sequence: parse_number(fields[0], "record sequence")?,
        occurred_at_unix_ms: parse_number(fields[1], "record timestamp")?,
        event,
    })
}

fn parse_number<T>(value: &str, label: &str) -> DiagnosticResult<T>
where
    T: std::str::FromStr,
{
    value
        .parse()
        .map_err(|_| DiagnosticError::Storage(format!("invalid {label} in diagnostics storage")))
}

fn validate_snapshot(snapshot: &DiagnosticSnapshot) -> DiagnosticResult<()> {
    let mut previous = 0;
    for record in &snapshot.records {
        if record.sequence == 0
            || record.sequence <= previous
            || record.sequence >= snapshot.next_sequence
        {
            return Err(DiagnosticError::Storage(
                "invalid diagnostic record sequence".to_owned(),
            ));
        }
        previous = record.sequence;
    }
    if snapshot.next_sequence == 0 {
        return Err(DiagnosticError::Storage(
            "invalid diagnostic next sequence".to_owned(),
        ));
    }
    Ok(())
}

fn encode_hex(value: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn decode_hex(value: &str) -> DiagnosticResult<String> {
    if !value.len().is_multiple_of(2) {
        return Err(DiagnosticError::Storage(
            "invalid diagnostic message encoding".to_owned(),
        ));
    }
    let (pairs, remainder) = value.as_bytes().as_chunks::<2>();
    debug_assert!(remainder.is_empty());
    let bytes: DiagnosticResult<Vec<u8>> = pairs
        .iter()
        .map(|pair| {
            let high = hex_value(pair[0])?;
            let low = hex_value(pair[1])?;
            Ok((high << 4) | low)
        })
        .collect();
    String::from_utf8(
        bytes.map_err(|_| {
            DiagnosticError::Storage("invalid diagnostic message encoding".to_owned())
        })?,
    )
    .map_err(|_| DiagnosticError::Storage("diagnostic message is not UTF-8".to_owned()))
}

fn hex_value(value: u8) -> DiagnosticResult<u8> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(DiagnosticError::Storage(
            "invalid diagnostic message encoding".to_owned(),
        )),
    }
}

fn redact_credentials(value: &str) -> String {
    const LABELS: [&str; 6] = [
        "password=",
        "passwd=",
        "token=",
        "secret=",
        "authorization:",
        "cookie=",
    ];
    let mut remaining = value;
    let mut output = String::new();
    loop {
        let lowercase = remaining.to_ascii_lowercase();
        let found = LABELS
            .iter()
            .filter_map(|label| lowercase.find(label).map(|index| (index, *label)))
            .min_by_key(|(index, _)| *index);
        let Some((index, label)) = found else {
            output.push_str(remaining);
            break;
        };
        output.push_str(&remaining[..index + label.len()]);
        output.push_str("[redacted]");
        let after_label = &remaining[index + label.len()..];
        if label == "authorization:" {
            break;
        }
        let value_start = after_label
            .find(|character: char| !character.is_whitespace())
            .unwrap_or(after_label.len());
        let value = &after_label[value_start..];
        let value_end = value
            .find(|character: char| character.is_whitespace() || matches!(character, ',' | ';'))
            .unwrap_or(value.len());
        remaining = &value[value_end..];
    }
    output
}

fn redact_home_paths(value: &str) -> String {
    let mut output = String::new();
    let mut remaining = value;
    loop {
        let home_index = remaining
            .find("/home/")
            .into_iter()
            .chain(remaining.find("/Users/"))
            .min();
        let Some(index) = home_index else {
            output.push_str(remaining);
            break;
        };
        output.push_str(&remaining[..index]);
        output.push_str("[redacted-path]");
        let after_path = &remaining[index..];
        let end = after_path
            .find(|character: char| character.is_whitespace() || matches!(character, ',' | ';'))
            .unwrap_or(after_path.len());
        remaining = &after_path[end..];
    }
    output
}

fn io_error(action: &str, error: io::Error) -> DiagnosticError {
    DiagnosticError::Storage(format!("could not {action}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{
        DiagnosticCode, DiagnosticEvent, DiagnosticRetention, DiagnosticSeverity, DiagnosticSource,
        DiagnosticStore, DiagnosticsService, FileDiagnosticStore, MemoryDiagnosticStore,
        SolComponent,
    };
    use sol_app::AppId;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn service_attributes_redacts_and_bounds_memory_history() {
        let service = DiagnosticsService::new(
            MemoryDiagnosticStore::new(),
            DiagnosticRetention::new(2).expect("positive retention should construct"),
        )
        .expect("empty memory store should initialize");
        let app = AppId::parse("org.sol.files").expect("test app ID should be valid");
        service
            .record(DiagnosticEvent::new(
                DiagnosticSource::Application(app),
                DiagnosticSeverity::Error,
                DiagnosticCode::StorageFailure,
            ))
            .expect("event should persist");
        service
            .record(DiagnosticEvent::new(
                DiagnosticSource::Component(SolComponent::Shell),
                DiagnosticSeverity::Warning,
                DiagnosticCode::TransportFailure,
            ))
            .expect("event should persist");
        let newest = service
            .record(
                DiagnosticEvent::new(
                    DiagnosticSource::Component(SolComponent::Compositor),
                    DiagnosticSeverity::Fatal,
                    DiagnosticCode::ProcessCrash,
                )
                .with_message("token=top-secret while reading /home/ava/private/report"),
            )
            .expect("event should persist");

        let records = service.records().expect("records should be available");
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].sequence, 2);
        assert_eq!(records[1], newest);
        assert_eq!(
            records[1].event.source,
            DiagnosticSource::Component(SolComponent::Compositor)
        );

        let persisted = service
            .store()
            .load()
            .expect("memory store should load")
            .expect("service should write through to storage");
        assert_eq!(persisted.records.len(), 2);
        let newest_message = persisted.records[1]
            .event
            .message()
            .expect("newest retained record has a summary")
            .as_str();
        assert!(!newest_message.contains("top-secret"));
        assert!(!newest_message.contains("/home/ava"));
        assert!(newest_message.contains("[redacted]"));
        assert!(newest_message.contains("[redacted-path]"));
    }

    #[test]
    fn file_store_round_trips_only_redacted_typed_data() {
        let path = temporary_test_path();
        let service = DiagnosticsService::new(
            FileDiagnosticStore::new(&path),
            DiagnosticRetention::new(3).expect("positive retention should construct"),
        )
        .expect("new file store should initialize");
        let expected = service
            .record(
                DiagnosticEvent::new(
                    DiagnosticSource::Component(SolComponent::InputMethod),
                    DiagnosticSeverity::Error,
                    DiagnosticCode::ProtocolViolation,
                )
                .with_message("authorization: bearer-secret"),
            )
            .expect("event should persist");

        let reloaded = DiagnosticsService::new(
            FileDiagnosticStore::new(&path),
            DiagnosticRetention::new(3).expect("positive retention should construct"),
        )
        .expect("persisted store should reload")
        .records()
        .expect("reloaded records should be available");
        assert_eq!(reloaded, vec![expected]);
        assert!(
            !reloaded[0]
                .event
                .message()
                .expect("reloaded record has a summary")
                .as_str()
                .contains("bearer-secret")
        );
        let contents = fs::read_to_string(&path).expect("diagnostics file should be readable");
        assert!(!contents.contains("bearer-secret"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path)
                .expect("diagnostics file metadata should be readable")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
        fs::remove_file(path).expect("test diagnostics file should be removable");
    }

    #[test]
    fn zero_retention_is_rejected() {
        assert!(DiagnosticRetention::new(0).is_err());
    }

    fn temporary_test_path() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "sol-diagnostics-test-{}-{nonce}.log",
            std::process::id()
        ))
    }
}
