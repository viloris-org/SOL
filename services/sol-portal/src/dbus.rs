//! D-Bus adapter for typed portal authorization requests.
//!
//! This adapter returns authorization decisions, not authorization tokens or
//! captured data. A future server-side document/screencast adapter must still
//! consume the private `PortalAuthorization` before performing any work.

use std::sync::Arc;

use sol_system::{AppId, SystemActionApi};
use zbus::blocking::{Connection, Proxy, connection::Builder};

use crate::{PortalOutcome, PortalRequest, PortalService};

pub const SERVICE_NAME: &str = "org.sol.Portal1";
pub const OBJECT_PATH: &str = "/org/sol/Portal1";
pub const INTERFACE_NAME: &str = "org.sol.Portal1";

type WireOutcome = (String, u64, u64, String);

/// A client-visible authorization decision with no executable token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortalDbusOutcome {
    Authorized {
        request_id: u64,
    },
    AwaitingUserConsent {
        request_id: u64,
        consent_id: u64,
        rationale: String,
    },
    Denied,
}

pub struct PortalDbusService<A> {
    portal: Arc<PortalService<A>>,
}

impl<A: SystemActionApi> PortalDbusService<A> {
    #[must_use]
    pub fn new(portal: PortalService<A>) -> Self {
        Self {
            portal: Arc::new(portal),
        }
    }

    fn request_portal(
        &self,
        caller: String,
        kind: String,
        uri: String,
    ) -> zbus::fdo::Result<WireOutcome> {
        let caller = AppId::parse(caller)
            .map_err(|error| zbus::fdo::Error::InvalidArgs(error.to_string()))?;
        let request = match kind.as_str() {
            "open-document" => {
                if uri.trim().is_empty() {
                    return Err(zbus::fdo::Error::InvalidArgs(
                        "document URI must not be empty".to_owned(),
                    ));
                }
                PortalRequest::OpenDocument { uri }
            }
            "screen-capture" => PortalRequest::ScreenCapture,
            _ => {
                return Err(zbus::fdo::Error::InvalidArgs(
                    "portal kind must be open-document or screen-capture".to_owned(),
                ));
            }
        };
        self.portal
            .request(caller, request)
            .map(outcome_to_wire)
            .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
    }
}

#[zbus::interface(name = "org.sol.Portal1")]
impl<A: SystemActionApi + 'static> PortalDbusService<A> {
    fn request(&self, caller: String, kind: String, uri: String) -> zbus::fdo::Result<WireOutcome> {
        self.request_portal(caller, kind, uri)
    }
}

pub fn serve_session<A: SystemActionApi + 'static>(
    portal: PortalService<A>,
) -> Result<Connection, String> {
    Builder::session()
        .map_err(bus_error)?
        .name(SERVICE_NAME)
        .map_err(bus_error)?
        .serve_at(OBJECT_PATH, PortalDbusService::new(portal))
        .map_err(bus_error)?
        .build()
        .map_err(bus_error)
}

pub struct PortalDbusProxy {
    proxy: Proxy<'static>,
}

impl std::fmt::Debug for PortalDbusProxy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PortalDbusProxy")
            .finish_non_exhaustive()
    }
}

impl PortalDbusProxy {
    pub fn connect() -> Result<Self, String> {
        let connection = Connection::session().map_err(bus_error)?;
        Proxy::new_owned(connection, SERVICE_NAME, OBJECT_PATH, INTERFACE_NAME)
            .map(|proxy| Self { proxy })
            .map_err(bus_error)
    }

    pub fn request(
        &self,
        caller: &AppId,
        request: &PortalRequest,
    ) -> Result<PortalDbusOutcome, String> {
        let (kind, uri) = match request {
            PortalRequest::OpenDocument { uri } => ("open-document", uri.as_str()),
            PortalRequest::ScreenCapture => ("screen-capture", ""),
        };
        let outcome: WireOutcome = self
            .proxy
            .call("Request", &(caller.as_str(), kind, uri))
            .map_err(bus_error)?;
        outcome_from_wire(outcome)
    }
}

fn outcome_to_wire(outcome: PortalOutcome) -> WireOutcome {
    match outcome {
        PortalOutcome::Authorized(authorization) => (
            "authorized".to_owned(),
            authorization.request_id(),
            0,
            String::new(),
        ),
        PortalOutcome::AwaitingUserConsent(consent) => (
            "awaiting-user-consent".to_owned(),
            consent.request_id.get(),
            consent.consent_id.get(),
            consent.rationale,
        ),
        PortalOutcome::Denied => ("denied".to_owned(), 0, 0, String::new()),
    }
}

fn outcome_from_wire(outcome: WireOutcome) -> Result<PortalDbusOutcome, String> {
    let (status, request_id, consent_id, rationale) = outcome;
    match status.as_str() {
        "authorized" => Ok(PortalDbusOutcome::Authorized { request_id }),
        "awaiting-user-consent" => Ok(PortalDbusOutcome::AwaitingUserConsent {
            request_id,
            consent_id,
            rationale,
        }),
        "denied" => Ok(PortalDbusOutcome::Denied),
        _ => Err(format!("portal D-Bus returned unknown status {status}")),
    }
}

fn bus_error(error: impl std::fmt::Display) -> String {
    format!("portal D-Bus: {error}")
}

#[cfg(test)]
mod tests {
    use super::{PortalDbusOutcome, outcome_from_wire};

    #[test]
    fn wire_decision_never_contains_an_authorization_token() {
        assert_eq!(
            outcome_from_wire(("authorized".to_owned(), 17, 0, String::new())),
            Ok(PortalDbusOutcome::Authorized { request_id: 17 })
        );
    }
}
