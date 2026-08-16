//! Typed portal request boundary for sensitive document and capture flows.
//!
//! This crate deliberately mediates only typed intent and the existing
//! [`sol_system::SystemActionApi`] authorization result. It does not open a
//! file chooser, capture pixels, create a PipeWire stream, or speak the XDG
//! Desktop Portal D-Bus protocol. Those concrete adapters must consume a
//! [`PortalAuthorization`] rather than bypassing this boundary.

pub mod dbus;

use sol_system::{
    ActionAuthorization, ActionError, ActionSource, AppId, SystemAction, SystemActionApi,
    SystemActionRequest, SystemActionResult, UserConsentRequest,
};
use std::error::Error;
use std::fmt;

/// A portal operation with no arbitrary command or argument-vector escape hatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortalRequest {
    /// Ask the user to select a document that a caller may open.
    OpenDocument {
        /// Non-empty URI chosen or supplied through a future portal UI.
        uri: String,
    },
    /// Ask to capture a display through a future screencast/recording adapter.
    ScreenCapture,
}

/// A request whose authorization may be consumed by exactly one matching adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortalAuthorization {
    authorization: ActionAuthorization,
}

impl PortalAuthorization {
    /// Return the validated caller that received this authorization.
    #[must_use]
    pub fn caller(&self) -> &AppId {
        &self.authorization.request.caller
    }

    /// Return the typed portal request the adapter may perform.
    #[must_use]
    pub fn request(&self) -> PortalRequest {
        portal_request_from_action(&self.authorization.request.action)
            .expect("portal authorizations contain only portal actions")
    }

    /// Return the system-wide correlation identifier for audit/UI association.
    #[must_use]
    pub const fn request_id(&self) -> u64 {
        self.authorization.request_id.get()
    }
}

/// The portal response after the shared permission layer evaluated a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortalOutcome {
    /// A matching platform adapter may perform the typed portal operation.
    Authorized(PortalAuthorization),
    /// A trusted consent UI must decide before an adapter sees authorization.
    AwaitingUserConsent(UserConsentRequest),
    /// The shared permission layer denied the operation.
    Denied,
}

/// Failure returned while evaluating portal intent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortalError {
    /// The permission layer failed to evaluate or audit a valid typed request.
    Authorization(ActionError),
    /// An authorization adapter returned an action outside the portal catalog.
    InvalidAuthorization,
}

impl fmt::Display for PortalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Authorization(error) => error.fmt(formatter),
            Self::InvalidAuthorization => {
                formatter.write_str("portal received authorization for a non-portal action")
            }
        }
    }
}

impl Error for PortalError {}

impl From<ActionError> for PortalError {
    fn from(error: ActionError) -> Self {
        Self::Authorization(error)
    }
}

/// Result returned by portal request evaluation.
pub type PortalResult<T> = Result<T, PortalError>;

/// Portal facade that preserves the system action/permission boundary.
pub struct PortalService<A> {
    actions: A,
}

impl<A: SystemActionApi> PortalService<A> {
    /// Construct a portal facade backed by the typed authorization API.
    #[must_use]
    pub const fn new(actions: A) -> Self {
        Self { actions }
    }

    /// Submit a typed document or capture request on behalf of `caller`.
    ///
    /// This does not perform an external operation. Only
    /// [`PortalOutcome::Authorized`] may be handed to a concrete adapter.
    pub fn request(&self, caller: AppId, request: PortalRequest) -> PortalResult<PortalOutcome> {
        let action = action_from_portal_request(&request);
        let result = self.actions.request(SystemActionRequest {
            caller,
            source: ActionSource::Portal,
            action,
        })?;
        match result {
            SystemActionResult::Authorized(authorization) => {
                if portal_request_from_action(&authorization.request.action).is_none() {
                    return Err(PortalError::InvalidAuthorization);
                }
                Ok(PortalOutcome::Authorized(PortalAuthorization {
                    authorization,
                }))
            }
            SystemActionResult::AwaitingUserConsent(consent) => {
                Ok(PortalOutcome::AwaitingUserConsent(consent))
            }
            SystemActionResult::Denied { .. } => Ok(PortalOutcome::Denied),
        }
    }
}

fn action_from_portal_request(request: &PortalRequest) -> SystemAction {
    match request {
        PortalRequest::OpenDocument { uri } => SystemAction::OpenDocument { uri: uri.clone() },
        PortalRequest::ScreenCapture => SystemAction::RequestScreenCapture,
    }
}

fn portal_request_from_action(action: &SystemAction) -> Option<PortalRequest> {
    match action {
        SystemAction::OpenDocument { uri } => {
            Some(PortalRequest::OpenDocument { uri: uri.clone() })
        }
        SystemAction::RequestScreenCapture => Some(PortalRequest::ScreenCapture),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sol_system::{
        ActionResult, DefaultDenyPolicy, MemoryActionAuditStore, PermissionGrant, PermissionKey,
        PermissionStore, SystemActionService, SystemCapability,
    };

    #[derive(Debug, Default)]
    struct AllowPortalStore;

    impl PermissionStore for AllowPortalStore {
        fn get(&self, key: &PermissionKey) -> ActionResult<Option<PermissionGrant>> {
            let grant = matches!(
                key.capability,
                SystemCapability::OpenDocuments | SystemCapability::ScreenCapture
            )
            .then_some(PermissionGrant::Allow);
            Ok(grant)
        }

        fn set(&self, _key: PermissionKey, _grant: PermissionGrant) -> ActionResult<()> {
            Ok(())
        }

        fn revoke(&self, _key: &PermissionKey) -> ActionResult<bool> {
            Ok(false)
        }
    }

    fn caller() -> AppId {
        AppId::parse("org.sol.portal-test").expect("test caller must be valid")
    }

    #[test]
    fn authorized_portal_requests_retain_typed_operation_and_caller() {
        let actions = SystemActionService::new(
            DefaultDenyPolicy,
            AllowPortalStore,
            MemoryActionAuditStore::default(),
        );
        let portal = PortalService::new(actions);
        let caller = caller();

        let PortalOutcome::Authorized(document) = portal
            .request(
                caller.clone(),
                PortalRequest::OpenDocument {
                    uri: "file:///tmp/report.txt".to_owned(),
                },
            )
            .expect("typed document request should evaluate")
        else {
            panic!("document request should be authorized");
        };
        assert_eq!(document.caller(), &caller);
        assert_eq!(
            document.request(),
            PortalRequest::OpenDocument {
                uri: "file:///tmp/report.txt".to_owned()
            }
        );
        assert!(document.request_id() > 0);

        assert!(matches!(
            portal
                .request(caller, PortalRequest::ScreenCapture)
                .unwrap(),
            PortalOutcome::Authorized(_)
        ));
    }

    #[test]
    fn default_deny_never_hands_portal_work_to_an_adapter() {
        let actions = SystemActionService::new(
            DefaultDenyPolicy,
            sol_system::MemoryPermissionStore::default(),
            MemoryActionAuditStore::default(),
        );
        let portal = PortalService::new(actions);

        assert!(matches!(
            portal
                .request(caller(), PortalRequest::ScreenCapture)
                .unwrap(),
            PortalOutcome::Denied
        ));
    }
}
