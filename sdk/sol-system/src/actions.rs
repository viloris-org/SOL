//! Typed, auditable system-action authorization.
//!
//! This module intentionally authorizes intent only.  A compositor, portal,
//! settings daemon, or notification daemon must perform the actual operation
//! after it receives an [`ActionAuthorization`].  This prevents a search or AI
//! client from gaining arbitrary shell execution through this API.

use crate::{AppId, NotificationActionId, NotificationId, OutputVolume};
use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const PERMISSION_STORE_VERSION: u32 = 1;

/// A stable action family understood by the system-action catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ActionKind {
    /// Start a known application.
    LaunchApplication,
    /// Search an indexed system source.
    Search,
    /// Change the output volume from Quick Settings.
    SetOutputVolume,
    /// Change output mute state from Quick Settings.
    SetOutputMuted,
    /// Invoke a previously advertised notification action.
    InvokeNotificationAction,
    /// Ask a portal for screen-capture authorization.
    RequestScreenCapture,
    /// Ask the document portal to open a URI selected by the user.
    OpenDocument,
}

/// A permission capability granted to a caller, rather than to an untyped command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SystemCapability {
    /// Start applications through the shell launcher.
    LaunchApplications,
    /// Query the system search index.
    Search,
    /// Change quick settings such as output volume.
    ChangeQuickSettings,
    /// Invoke notification actions the caller is allowed to handle.
    InvokeNotificationActions,
    /// Request screen capture through a desktop portal.
    ScreenCapture,
    /// Open a document through a desktop portal.
    OpenDocuments,
}

impl SystemCapability {
    /// Return the stable on-disk spelling for a capability grant.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LaunchApplications => "launch-applications",
            Self::Search => "search",
            Self::ChangeQuickSettings => "change-quick-settings",
            Self::InvokeNotificationActions => "invoke-notification-actions",
            Self::ScreenCapture => "screen-capture",
            Self::OpenDocuments => "open-documents",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "launch-applications" => Some(Self::LaunchApplications),
            "search" => Some(Self::Search),
            "change-quick-settings" => Some(Self::ChangeQuickSettings),
            "invoke-notification-actions" => Some(Self::InvokeNotificationActions),
            "screen-capture" => Some(Self::ScreenCapture),
            "open-documents" => Some(Self::OpenDocuments),
            _ => None,
        }
    }
}

/// A typed system intent.  This catalog contains no arbitrary command or shell string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemAction {
    /// Launch a registered application.
    LaunchApplication { app_id: AppId },
    /// Search a system index with a non-empty query.
    Search { query: String },
    /// Set an already validated output volume.
    SetOutputVolume { volume: OutputVolume },
    /// Set output mute state.
    SetOutputMuted { muted: bool },
    /// Invoke one of a notification's declared actions.
    InvokeNotificationAction {
        /// Notification owning the action.
        notification_id: NotificationId,
        /// Action selected from that notification.
        action_id: NotificationActionId,
    },
    /// Request screen capture through a portal implementation.
    RequestScreenCapture,
    /// Open a non-empty URI through the document portal.
    OpenDocument { uri: String },
}

impl SystemAction {
    /// Return the action family.
    #[must_use]
    pub const fn kind(&self) -> ActionKind {
        match self {
            Self::LaunchApplication { .. } => ActionKind::LaunchApplication,
            Self::Search { .. } => ActionKind::Search,
            Self::SetOutputVolume { .. } => ActionKind::SetOutputVolume,
            Self::SetOutputMuted { .. } => ActionKind::SetOutputMuted,
            Self::InvokeNotificationAction { .. } => ActionKind::InvokeNotificationAction,
            Self::RequestScreenCapture => ActionKind::RequestScreenCapture,
            Self::OpenDocument { .. } => ActionKind::OpenDocument,
        }
    }

    fn validate(&self) -> Result<(), ActionError> {
        match self {
            Self::Search { query } if query.trim().is_empty() => Err(ActionError::invalid_request(
                "search query must not be empty",
            )),
            Self::OpenDocument { uri } if uri.trim().is_empty() => Err(
                ActionError::invalid_request("document URI must not be empty"),
            ),
            _ => Ok(()),
        }
    }
}

/// Maps stable action kinds to their least-privilege capabilities.
pub struct SystemActionCatalog;

impl SystemActionCatalog {
    /// Return the capability required for an action.
    #[must_use]
    pub const fn capability(action: &SystemAction) -> SystemCapability {
        match action.kind() {
            ActionKind::LaunchApplication => SystemCapability::LaunchApplications,
            ActionKind::Search => SystemCapability::Search,
            ActionKind::SetOutputVolume | ActionKind::SetOutputMuted => {
                SystemCapability::ChangeQuickSettings
            }
            ActionKind::InvokeNotificationAction => SystemCapability::InvokeNotificationActions,
            ActionKind::RequestScreenCapture => SystemCapability::ScreenCapture,
            ActionKind::OpenDocument => SystemCapability::OpenDocuments,
        }
    }
}

/// The surface that originated an action request; retained for auditability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionSource {
    /// Shell dock or launcher.
    ShellLauncher,
    /// System search UI.
    Search,
    /// Quick Settings UI.
    QuickSettings,
    /// Notification center UI.
    Notifications,
    /// XDG desktop portal-facing UI.
    Portal,
    /// Accessibility tooling.
    Accessibility,
    /// User-authored automation.
    Automation,
    /// AI/voice intent that must remain constrained to this catalog.
    AiOrVoice,
}

/// An attributed request to authorize an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemActionRequest {
    /// Validated application identity of the caller.
    pub caller: AppId,
    /// UI or service surface that originated the request.
    pub source: ActionSource,
    /// Intent to authorize.
    pub action: SystemAction,
}

/// Opaque identifier assigned by the authorization service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActionRequestId(u64);

impl ActionRequestId {
    /// Return the identifier for correlation with audit records.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Opaque identifier handed to a trusted consent UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConsentId(u64);

impl ConsentId {
    /// Return the identifier for correlating a displayed consent prompt.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A caller and capability pair used as a permission-store key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PermissionKey {
    /// The application receiving the grant.
    pub caller: AppId,
    /// The capability controlled by the grant.
    pub capability: SystemCapability,
}

impl PermissionKey {
    /// Construct a caller-scoped permission key.
    #[must_use]
    pub const fn new(caller: AppId, capability: SystemCapability) -> Self {
        Self { caller, capability }
    }
}

/// A stored permission decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionGrant {
    /// Authorize this capability for the caller.
    Allow,
    /// Explicitly deny this capability for the caller.
    Deny,
}

impl PermissionGrant {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "allow" => Some(Self::Allow),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }
}

/// A reason an action was not authorized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionDenial {
    /// No stored grant existed and the default policy is deny.
    DefaultDeny,
    /// A stored caller-scoped grant denied the action.
    StoredDeny,
    /// The user rejected a consent prompt.
    UserDenied,
    /// A policy rejected the request with its own stable reason.
    Policy(String),
}

/// Decision that the policy makes for an ungranted action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    /// Do not authorize the action.
    Deny(ActionDenial),
    /// Stop at an explicit user-consent boundary.
    RequireUserConsent {
        /// Explanation suitable for a system consent surface.
        rationale: String,
    },
}

/// A completed authorization decision, preserved in the audit log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecision {
    /// The request may be handed to a concrete system adapter.
    Authorized,
    /// The request must not be handed to an adapter.
    Denied(ActionDenial),
    /// The request awaits an explicit user decision.
    AwaitingUserConsent,
}

/// Authorization token describing the request an adapter may execute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionAuthorization {
    /// Service-assigned correlation identifier.
    pub request_id: ActionRequestId,
    /// Original attributed request.
    pub request: SystemActionRequest,
    /// Capability checked before authorization.
    pub capability: SystemCapability,
}

/// A request that must be rendered by a trusted consent UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserConsentRequest {
    /// Opaque ID that must be returned to resolve this exact prompt.
    pub consent_id: ConsentId,
    /// Service-assigned correlation identifier.
    pub request_id: ActionRequestId,
    /// Caller requiring consent.
    pub caller: AppId,
    /// Original surface that submitted the request.
    pub source: ActionSource,
    /// Capability being requested.
    pub capability: SystemCapability,
    /// The exact action that would become authorized.
    pub action: SystemAction,
    /// Policy-provided explanation for the user.
    pub rationale: String,
}

/// User response supplied only by a trusted system consent surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentDecision {
    /// Authorize this request only; do not persist a grant.
    AllowOnce,
    /// Authorize this request and persist a caller-scoped allow grant.
    AllowAlways,
    /// Deny this request and persist a caller-scoped deny grant.
    Deny,
}

/// Result of evaluating an action request.  None of these executes an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemActionResult {
    /// A concrete adapter may perform the described action.
    Authorized(ActionAuthorization),
    /// The action remains blocked.
    Denied {
        /// Service-assigned correlation identifier.
        request_id: ActionRequestId,
        /// Original request for audit correlation.
        request: SystemActionRequest,
        /// Denial reason.
        reason: ActionDenial,
    },
    /// A trusted user-consent surface must decide before anything can execute.
    AwaitingUserConsent(UserConsentRequest),
}

/// Error raised by the authorization boundary or one of its stores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionError {
    /// Request data does not satisfy the typed action contract.
    InvalidRequest(String),
    /// The consent identifier is missing or has already been resolved.
    UnknownConsent(ConsentId),
    /// A pluggable store could not complete its operation.
    Store(String),
    /// An audit log could not persist a required decision record.
    Audit(String),
}

impl ActionError {
    fn invalid_request(message: impl Into<String>) -> Self {
        Self::InvalidRequest(message.into())
    }

    /// Construct a permission-store failure.
    #[must_use]
    pub fn store(message: impl Into<String>) -> Self {
        Self::Store(message.into())
    }

    /// Construct an audit-store failure.
    #[must_use]
    pub fn audit(message: impl Into<String>) -> Self {
        Self::Audit(message.into())
    }
}

impl fmt::Display for ActionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest(message) => write!(formatter, "invalid action request: {message}"),
            Self::UnknownConsent(id) => write!(
                formatter,
                "unknown or resolved consent request {}",
                id.get()
            ),
            Self::Store(message) => write!(formatter, "permission store failure: {message}"),
            Self::Audit(message) => write!(formatter, "action audit failure: {message}"),
        }
    }
}

impl Error for ActionError {}

/// Result returned by the system-action authorization API.
pub type ActionResult<T> = Result<T, ActionError>;

/// Storage boundary for caller-scoped capability grants.
pub trait PermissionStore: Send + Sync {
    /// Look up a stored grant, if any.
    fn get(&self, key: &PermissionKey) -> ActionResult<Option<PermissionGrant>>;
    /// Persist an explicit grant or denial.
    fn set(&self, key: PermissionKey, grant: PermissionGrant) -> ActionResult<()>;
    /// Remove a grant. Returns whether a grant existed.
    fn revoke(&self, key: &PermissionKey) -> ActionResult<bool>;
}

/// Default in-memory permission store, suitable for deterministic tests and fixtures.
#[derive(Debug, Default)]
pub struct MemoryPermissionStore {
    grants: Mutex<BTreeMap<PermissionKey, PermissionGrant>>,
}

impl PermissionStore for MemoryPermissionStore {
    fn get(&self, key: &PermissionKey) -> ActionResult<Option<PermissionGrant>> {
        Ok(self
            .grants
            .lock()
            .map_err(|error| ActionError::store(format!("permission lock poisoned: {error}")))?
            .get(key)
            .copied())
    }

    fn set(&self, key: PermissionKey, grant: PermissionGrant) -> ActionResult<()> {
        self.grants
            .lock()
            .map_err(|error| ActionError::store(format!("permission lock poisoned: {error}")))?
            .insert(key, grant);
        Ok(())
    }

    fn revoke(&self, key: &PermissionKey) -> ActionResult<bool> {
        Ok(self
            .grants
            .lock()
            .map_err(|error| ActionError::store(format!("permission lock poisoned: {error}")))?
            .remove(key)
            .is_some())
    }
}

/// A daemon-owned, atomically replaced store for caller-scoped permission grants.
///
/// Each instance serializes its own read-modify-write operations. Production
/// setup should give one settings/security daemon ownership of a store path;
/// cross-process coordination and trusted consent UI remain separate work.
#[derive(Debug)]
pub struct FilePermissionStore {
    path: PathBuf,
    guard: Mutex<()>,
}

impl FilePermissionStore {
    /// Create a permission store backed by a private file at `path`.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            guard: Mutex::new(()),
        }
    }

    /// Return the daemon-owned file path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn load(&self) -> ActionResult<BTreeMap<PermissionKey, PermissionGrant>> {
        let contents = match fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
            Err(error) => {
                return Err(ActionError::store(format!(
                    "read permission store: {error}"
                )));
            }
        };
        parse_permission_store(&contents)
    }

    fn save(&self, grants: &BTreeMap<PermissionKey, PermissionGrant>) -> ActionResult<()> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)
            .map_err(|error| ActionError::store(format!("create permission directory: {error}")))?;
        let temporary = permission_temporary_path(&self.path)?;
        let result = write_permission_store(&temporary, grants);
        if let Err(error) = result {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
        fs::rename(&temporary, &self.path)
            .map_err(|error| ActionError::store(format!("replace permission store: {error}")))?;
        restrict_permissions(&self.path)
            .map_err(|error| ActionError::store(format!("restrict permission store: {error}")))
    }
}

impl PermissionStore for FilePermissionStore {
    fn get(&self, key: &PermissionKey) -> ActionResult<Option<PermissionGrant>> {
        let _guard = self.guard.lock().map_err(|error| {
            ActionError::store(format!("permission file lock poisoned: {error}"))
        })?;
        Ok(self.load()?.get(key).copied())
    }

    fn set(&self, key: PermissionKey, grant: PermissionGrant) -> ActionResult<()> {
        let _guard = self.guard.lock().map_err(|error| {
            ActionError::store(format!("permission file lock poisoned: {error}"))
        })?;
        let mut grants = self.load()?;
        grants.insert(key, grant);
        self.save(&grants)
    }

    fn revoke(&self, key: &PermissionKey) -> ActionResult<bool> {
        let _guard = self.guard.lock().map_err(|error| {
            ActionError::store(format!("permission file lock poisoned: {error}"))
        })?;
        let mut grants = self.load()?;
        let removed = grants.remove(key).is_some();
        if removed {
            self.save(&grants)?;
        }
        Ok(removed)
    }
}

fn parse_permission_store(
    contents: &str,
) -> ActionResult<BTreeMap<PermissionKey, PermissionGrant>> {
    let mut version = None;
    let mut grants = BTreeMap::new();
    for line in contents.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(value) = line.strip_prefix("version=") {
            version = Some(
                value
                    .parse::<u32>()
                    .map_err(|_| ActionError::store("invalid permission store version"))?,
            );
            continue;
        }
        let mut fields = line.split('\t');
        let app_id = fields
            .next()
            .ok_or_else(|| ActionError::store("invalid permission store record"))?;
        let capability = fields
            .next()
            .and_then(SystemCapability::parse)
            .ok_or_else(|| ActionError::store("invalid permission capability"))?;
        let grant = fields
            .next()
            .and_then(PermissionGrant::parse)
            .ok_or_else(|| ActionError::store("invalid permission grant"))?;
        if fields.next().is_some() {
            return Err(ActionError::store("invalid permission store record"));
        }
        let caller = AppId::parse(app_id)
            .map_err(|_| ActionError::store("invalid permission application ID"))?;
        let key = PermissionKey::new(caller, capability);
        if grants.insert(key, grant).is_some() {
            return Err(ActionError::store("duplicate permission store record"));
        }
    }
    match version {
        Some(PERMISSION_STORE_VERSION) => Ok(grants),
        Some(version) => Err(ActionError::store(format!(
            "unsupported permission store version {version}"
        ))),
        None => Err(ActionError::store("permission store has no version")),
    }
}

fn permission_temporary_path(path: &Path) -> ActionResult<PathBuf> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ActionError::store("permission store path requires a UTF-8 file name"))?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| ActionError::store(format!("permission store clock failure: {error}")))?
        .as_nanos();
    Ok(path.with_file_name(format!(".{name}.tmp-{}-{nonce}", std::process::id())))
}

fn write_permission_store(
    path: &Path,
    grants: &BTreeMap<PermissionKey, PermissionGrant>,
) -> ActionResult<()> {
    let mut file = create_private_file(path).map_err(|error| {
        ActionError::store(format!("create permission temporary file: {error}"))
    })?;
    writeln!(
        file,
        "# SOL permission grants; format version {PERMISSION_STORE_VERSION}"
    )
    .map_err(|error| ActionError::store(format!("write permission store: {error}")))?;
    writeln!(file, "version={PERMISSION_STORE_VERSION}")
        .map_err(|error| ActionError::store(format!("write permission store: {error}")))?;
    for (key, grant) in grants {
        writeln!(
            file,
            "{}\t{}\t{}",
            key.caller,
            key.capability.as_str(),
            grant.as_str()
        )
        .map_err(|error| ActionError::store(format!("write permission store: {error}")))?;
    }
    file.sync_all()
        .map_err(|error| ActionError::store(format!("sync permission store: {error}")))
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

fn restrict_permissions(path: &Path) -> io::Result<()> {
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

/// Policy used only if the caller has no stored grant.
pub trait PermissionPolicy: Send + Sync {
    /// Decide whether an ungranted request is denied or needs user consent.
    fn decide(&self, request: &SystemActionRequest, capability: SystemCapability)
    -> PolicyDecision;
}

/// The production-safe baseline: every ungranted action is denied.
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultDenyPolicy;

impl PermissionPolicy for DefaultDenyPolicy {
    fn decide(
        &self,
        _request: &SystemActionRequest,
        _capability: SystemCapability,
    ) -> PolicyDecision {
        PolicyDecision::Deny(ActionDenial::DefaultDeny)
    }
}

/// A durable audit record for every completed or pending authorization decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionAuditRecord {
    /// Monotonic sequence assigned by the authorization service.
    pub sequence: u64,
    /// Correlation ID assigned to the request.
    pub request_id: ActionRequestId,
    /// Caller and action evaluated.
    pub request: SystemActionRequest,
    /// Capability that was checked.
    pub capability: SystemCapability,
    /// Decision made at this boundary.
    pub decision: PermissionDecision,
}

/// Persistence boundary for authorization records.
pub trait ActionAuditStore: Send + Sync {
    /// Persist one required authorization record.
    fn append(&self, record: ActionAuditRecord) -> ActionResult<()>;
    /// Return records in append order.
    fn records(&self) -> ActionResult<Vec<ActionAuditRecord>>;
}

/// Deterministic in-memory audit store for tests and headless fixtures.
#[derive(Debug, Default)]
pub struct MemoryActionAuditStore {
    records: Mutex<Vec<ActionAuditRecord>>,
}

impl ActionAuditStore for MemoryActionAuditStore {
    fn append(&self, record: ActionAuditRecord) -> ActionResult<()> {
        self.records
            .lock()
            .map_err(|error| ActionError::audit(format!("audit lock poisoned: {error}")))?
            .push(record);
        Ok(())
    }

    fn records(&self) -> ActionResult<Vec<ActionAuditRecord>> {
        Ok(self
            .records
            .lock()
            .map_err(|error| ActionError::audit(format!("audit lock poisoned: {error}")))?
            .clone())
    }
}

#[derive(Debug)]
struct PendingConsent {
    request_id: ActionRequestId,
    request: SystemActionRequest,
    capability: SystemCapability,
}

#[derive(Debug, Default)]
struct ServiceState {
    next_request_id: u64,
    next_consent_id: u64,
    next_audit_sequence: u64,
    pending: HashMap<ConsentId, PendingConsent>,
}

/// Evaluates typed action requests against grants and a default policy.
///
/// Calling this service never performs the requested system action.  A
/// platform adapter must separately consume [`ActionAuthorization`] after the
/// authorization record is persisted.
pub struct SystemActionService<P, S, A> {
    policy: P,
    permissions: S,
    audit: A,
    state: Mutex<ServiceState>,
}

/// Stable authorization boundary consumed by SOL clients and system adapters.
///
/// Implementations authorize typed intent only; they do not execute a system
/// action. This trait lets clients use an in-memory fake in tests without
/// depending on a concrete policy or persistence implementation.
pub trait SystemActionApi: Send + Sync {
    /// Evaluate an attributed request without executing the requested action.
    fn request(&self, request: SystemActionRequest) -> ActionResult<SystemActionResult>;

    /// Resolve a consent ID displayed by a trusted system consent surface.
    fn resolve_user_consent(
        &self,
        consent_id: ConsentId,
        decision: ConsentDecision,
    ) -> ActionResult<SystemActionResult>;

    /// Remove a caller-scoped capability grant.
    fn revoke(&self, caller: &AppId, capability: SystemCapability) -> ActionResult<bool>;

    /// Return authorization decisions in audit order.
    fn audit_records(&self) -> ActionResult<Vec<ActionAuditRecord>>;
}

impl<P, S, A> SystemActionService<P, S, A>
where
    P: PermissionPolicy,
    S: PermissionStore,
    A: ActionAuditStore,
{
    /// Construct an authorization service from explicit policy and stores.
    #[must_use]
    pub fn new(policy: P, permissions: S, audit: A) -> Self {
        Self {
            policy,
            permissions,
            audit,
            state: Mutex::new(ServiceState::default()),
        }
    }

    /// Evaluate an attributed request without executing the requested action.
    pub fn request(&self, request: SystemActionRequest) -> ActionResult<SystemActionResult> {
        request.action.validate()?;
        let capability = SystemActionCatalog::capability(&request.action);
        let request_id = self.next_request_id()?;
        let key = PermissionKey::new(request.caller.clone(), capability);

        let result = match self.permissions.get(&key)? {
            Some(PermissionGrant::Allow) => SystemActionResult::Authorized(ActionAuthorization {
                request_id,
                request: request.clone(),
                capability,
            }),
            Some(PermissionGrant::Deny) => {
                Self::denied(request_id, request.clone(), ActionDenial::StoredDeny)
            }
            None => match self.policy.decide(&request, capability) {
                PolicyDecision::Deny(reason) => Self::denied(request_id, request.clone(), reason),
                PolicyDecision::RequireUserConsent { rationale } => {
                    let consent_id =
                        self.insert_pending(request_id, request.clone(), capability)?;
                    SystemActionResult::AwaitingUserConsent(UserConsentRequest {
                        consent_id,
                        request_id,
                        caller: request.caller.clone(),
                        source: request.source,
                        capability,
                        action: request.action.clone(),
                        rationale,
                    })
                }
            },
        };
        self.audit_result(&result)?;
        Ok(result)
    }

    /// Resolve a prompt emitted by [`Self::request`].
    ///
    /// Only trusted system consent UI should call this method. The ID binds the
    /// decision to the originally audited caller, capability, and action.
    pub fn resolve_user_consent(
        &self,
        consent_id: ConsentId,
        decision: ConsentDecision,
    ) -> ActionResult<SystemActionResult> {
        let pending = self.remove_pending(consent_id)?;
        let key = PermissionKey::new(pending.request.caller.clone(), pending.capability);
        let result = match decision {
            ConsentDecision::AllowOnce => SystemActionResult::Authorized(ActionAuthorization {
                request_id: pending.request_id,
                request: pending.request,
                capability: pending.capability,
            }),
            ConsentDecision::AllowAlways => {
                self.permissions.set(key, PermissionGrant::Allow)?;
                SystemActionResult::Authorized(ActionAuthorization {
                    request_id: pending.request_id,
                    request: pending.request,
                    capability: pending.capability,
                })
            }
            ConsentDecision::Deny => {
                self.permissions.set(key, PermissionGrant::Deny)?;
                Self::denied(
                    pending.request_id,
                    pending.request,
                    ActionDenial::UserDenied,
                )
            }
        };
        self.audit_result(&result)?;
        Ok(result)
    }

    /// Remove a caller-scoped capability grant. Future requests use the policy again.
    pub fn revoke(&self, caller: &AppId, capability: SystemCapability) -> ActionResult<bool> {
        self.permissions
            .revoke(&PermissionKey::new(caller.clone(), capability))
    }

    /// Return completed and pending decision records in append order.
    pub fn audit_records(&self) -> ActionResult<Vec<ActionAuditRecord>> {
        self.audit.records()
    }

    fn denied(
        request_id: ActionRequestId,
        request: SystemActionRequest,
        reason: ActionDenial,
    ) -> SystemActionResult {
        SystemActionResult::Denied {
            request_id,
            request,
            reason,
        }
    }

    fn next_request_id(&self) -> ActionResult<ActionRequestId> {
        let mut state = self.lock_state()?;
        state.next_request_id += 1;
        Ok(ActionRequestId(state.next_request_id))
    }

    fn insert_pending(
        &self,
        request_id: ActionRequestId,
        request: SystemActionRequest,
        capability: SystemCapability,
    ) -> ActionResult<ConsentId> {
        let mut state = self.lock_state()?;
        state.next_consent_id += 1;
        let consent_id = ConsentId(state.next_consent_id);
        state.pending.insert(
            consent_id,
            PendingConsent {
                request_id,
                request,
                capability,
            },
        );
        Ok(consent_id)
    }

    fn remove_pending(&self, consent_id: ConsentId) -> ActionResult<PendingConsent> {
        self.lock_state()?
            .pending
            .remove(&consent_id)
            .ok_or(ActionError::UnknownConsent(consent_id))
    }

    fn audit_result(&self, result: &SystemActionResult) -> ActionResult<()> {
        let (request_id, request, capability, decision) = match result {
            SystemActionResult::Authorized(authorization) => (
                authorization.request_id,
                authorization.request.clone(),
                authorization.capability,
                PermissionDecision::Authorized,
            ),
            SystemActionResult::Denied {
                request_id,
                request,
                reason,
            } => (
                *request_id,
                request.clone(),
                SystemActionCatalog::capability(&request.action),
                PermissionDecision::Denied(reason.clone()),
            ),
            SystemActionResult::AwaitingUserConsent(consent) => (
                consent.request_id,
                SystemActionRequest {
                    caller: consent.caller.clone(),
                    source: consent.source,
                    action: consent.action.clone(),
                },
                consent.capability,
                PermissionDecision::AwaitingUserConsent,
            ),
        };
        let mut state = self.lock_state()?;
        state.next_audit_sequence += 1;
        let sequence = state.next_audit_sequence;
        drop(state);
        self.audit.append(ActionAuditRecord {
            sequence,
            request_id,
            request,
            capability,
            decision,
        })
    }

    fn lock_state(&self) -> ActionResult<std::sync::MutexGuard<'_, ServiceState>> {
        self.state
            .lock()
            .map_err(|error| ActionError::audit(format!("service state lock poisoned: {error}")))
    }
}

impl<P, S, A> SystemActionApi for SystemActionService<P, S, A>
where
    P: PermissionPolicy,
    S: PermissionStore,
    A: ActionAuditStore,
{
    fn request(&self, request: SystemActionRequest) -> ActionResult<SystemActionResult> {
        Self::request(self, request)
    }

    fn resolve_user_consent(
        &self,
        consent_id: ConsentId,
        decision: ConsentDecision,
    ) -> ActionResult<SystemActionResult> {
        Self::resolve_user_consent(self, consent_id, decision)
    }

    fn revoke(&self, caller: &AppId, capability: SystemCapability) -> ActionResult<bool> {
        Self::revoke(self, caller, capability)
    }

    fn audit_records(&self) -> ActionResult<Vec<ActionAuditRecord>> {
        Self::audit_records(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy)]
    struct PromptPolicy;

    impl PermissionPolicy for PromptPolicy {
        fn decide(
            &self,
            _request: &SystemActionRequest,
            _capability: SystemCapability,
        ) -> PolicyDecision {
            PolicyDecision::RequireUserConsent {
                rationale: "test prompt".to_owned(),
            }
        }
    }

    fn app_id() -> AppId {
        AppId::parse("org.sol.test-client").expect("valid test identity")
    }

    fn launch_request() -> SystemActionRequest {
        SystemActionRequest {
            caller: app_id(),
            source: ActionSource::ShellLauncher,
            action: SystemAction::LaunchApplication { app_id: app_id() },
        }
    }

    fn default_service()
    -> SystemActionService<DefaultDenyPolicy, MemoryPermissionStore, MemoryActionAuditStore> {
        SystemActionService::new(
            DefaultDenyPolicy,
            MemoryPermissionStore::default(),
            MemoryActionAuditStore::default(),
        )
    }

    #[test]
    fn default_policy_denies_and_audits_every_ungranted_action() {
        let service = default_service();
        let result = service
            .request(launch_request())
            .expect("authorization succeeds");

        assert!(matches!(
            result,
            SystemActionResult::Denied {
                reason: ActionDenial::DefaultDeny,
                ..
            }
        ));
        let records = service.audit_records().expect("audit is readable");
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].decision,
            PermissionDecision::Denied(ActionDenial::DefaultDeny)
        );
    }

    #[test]
    fn explicit_allow_authorizes_only_the_matching_caller_capability() {
        let service = default_service();
        let caller = app_id();
        service
            .permissions
            .set(
                PermissionKey::new(caller, SystemCapability::LaunchApplications),
                PermissionGrant::Allow,
            )
            .expect("in-memory grant succeeds");

        assert!(matches!(
            service
                .request(launch_request())
                .expect("authorization succeeds"),
            SystemActionResult::Authorized(_)
        ));
    }

    #[test]
    fn explicit_deny_overrides_the_safe_default_with_an_auditable_reason() {
        let service = default_service();
        service
            .permissions
            .set(
                PermissionKey::new(app_id(), SystemCapability::LaunchApplications),
                PermissionGrant::Deny,
            )
            .expect("in-memory grant succeeds");

        assert!(matches!(
            service
                .request(launch_request())
                .expect("authorization succeeds"),
            SystemActionResult::Denied {
                reason: ActionDenial::StoredDeny,
                ..
            }
        ));
    }

    #[test]
    fn revoking_a_grant_returns_the_caller_to_default_deny() {
        let service = default_service();
        let caller = app_id();
        service
            .permissions
            .set(
                PermissionKey::new(caller.clone(), SystemCapability::LaunchApplications),
                PermissionGrant::Allow,
            )
            .expect("in-memory grant succeeds");
        assert!(
            service
                .revoke(&caller, SystemCapability::LaunchApplications)
                .expect("revoke succeeds")
        );

        assert!(matches!(
            service
                .request(launch_request())
                .expect("authorization succeeds"),
            SystemActionResult::Denied {
                reason: ActionDenial::DefaultDeny,
                ..
            }
        ));
    }

    #[test]
    fn consent_boundary_never_authorizes_until_trusted_ui_resolves_it() {
        let service = SystemActionService::new(
            PromptPolicy,
            MemoryPermissionStore::default(),
            MemoryActionAuditStore::default(),
        );
        let pending = service.request(launch_request()).expect("request is valid");
        let SystemActionResult::AwaitingUserConsent(consent) = pending else {
            panic!("prompt policy must stop at consent boundary");
        };

        let result = service
            .resolve_user_consent(consent.consent_id, ConsentDecision::AllowOnce)
            .expect("trusted consent resolves");
        assert!(matches!(result, SystemActionResult::Authorized(_)));
        let records = service.audit_records().expect("audit is readable");
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].request.source, ActionSource::ShellLauncher);
        assert_eq!(records[1].request.source, ActionSource::ShellLauncher);
    }

    #[test]
    fn allow_always_from_consent_persists_a_caller_scoped_grant() {
        let service = SystemActionService::new(
            PromptPolicy,
            MemoryPermissionStore::default(),
            MemoryActionAuditStore::default(),
        );
        let SystemActionResult::AwaitingUserConsent(consent) =
            service.request(launch_request()).expect("request is valid")
        else {
            panic!("prompt policy must request consent");
        };
        assert!(matches!(
            service
                .resolve_user_consent(consent.consent_id, ConsentDecision::AllowAlways)
                .expect("consent resolution succeeds"),
            SystemActionResult::Authorized(_)
        ));

        assert!(matches!(
            service.request(launch_request()).expect("request is valid"),
            SystemActionResult::Authorized(_)
        ));
    }

    #[test]
    fn deny_from_consent_persists_and_blocks_future_requests() {
        let service = SystemActionService::new(
            PromptPolicy,
            MemoryPermissionStore::default(),
            MemoryActionAuditStore::default(),
        );
        let SystemActionResult::AwaitingUserConsent(consent) =
            service.request(launch_request()).expect("request is valid")
        else {
            panic!("prompt policy must request consent");
        };
        assert!(matches!(
            service
                .resolve_user_consent(consent.consent_id, ConsentDecision::Deny)
                .expect("consent resolution succeeds"),
            SystemActionResult::Denied {
                reason: ActionDenial::UserDenied,
                ..
            }
        ));
        assert!(matches!(
            service.request(launch_request()).expect("request is valid"),
            SystemActionResult::Denied {
                reason: ActionDenial::StoredDeny,
                ..
            }
        ));
    }

    #[test]
    fn invalid_untyped_like_payload_is_rejected_before_policy_or_audit() {
        let service = default_service();
        let request = SystemActionRequest {
            caller: app_id(),
            source: ActionSource::Search,
            action: SystemAction::Search {
                query: "   ".to_owned(),
            },
        };

        assert!(matches!(
            service.request(request),
            Err(ActionError::InvalidRequest(_))
        ));
        assert!(
            service
                .audit_records()
                .expect("audit is readable")
                .is_empty()
        );
    }

    #[test]
    fn catalog_maps_each_surface_action_to_a_least_privilege_capability() {
        assert_eq!(
            SystemActionCatalog::capability(&SystemAction::RequestScreenCapture),
            SystemCapability::ScreenCapture
        );
        assert_eq!(
            SystemActionCatalog::capability(&SystemAction::SetOutputMuted { muted: true }),
            SystemCapability::ChangeQuickSettings
        );
    }

    #[test]
    fn file_permission_store_round_trips_and_revokes_caller_scoped_grants() {
        let path = temporary_permission_store_path();
        let key = PermissionKey::new(app_id(), SystemCapability::ScreenCapture);
        let store = FilePermissionStore::new(&path);
        assert_eq!(store.get(&key).unwrap(), None);
        store.set(key.clone(), PermissionGrant::Allow).unwrap();
        assert_eq!(store.get(&key).unwrap(), Some(PermissionGrant::Allow));

        let reloaded = FilePermissionStore::new(&path);
        assert_eq!(reloaded.get(&key).unwrap(), Some(PermissionGrant::Allow));
        assert!(reloaded.revoke(&key).unwrap());
        assert_eq!(reloaded.get(&key).unwrap(), None);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        std::fs::remove_file(path).unwrap();
    }

    fn temporary_permission_store_path() -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "sol-permission-store-test-{}-{nonce}.conf",
            std::process::id()
        ))
    }
}
