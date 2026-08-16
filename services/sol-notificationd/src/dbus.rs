//! D-Bus transport for SOL's typed notification service.
//!
//! This is intentionally not a claim of `org.freedesktop.Notifications`
//! compatibility. The SOL contract carries validated reverse-DNS application
//! identities and explicit lifecycle reasons which are not available from an
//! unauthenticated generic notification request.

use std::sync::Arc;

use sol_system::{
    AppId, NotificationAction, NotificationActionId, NotificationActionInvocation, NotificationApi,
    NotificationDismissReason, NotificationError, NotificationId, NotificationLifecycle,
    NotificationQuery, NotificationRecord, NotificationRequest, NotificationResult,
    NotificationUrgency,
};
use zbus::blocking::{Connection, Proxy, connection::Builder};

use crate::{NotificationDaemon, NotificationStore};

pub const SERVICE_NAME: &str = "org.sol.Notifications1";
pub const OBJECT_PATH: &str = "/org/sol/Notifications1";
pub const INTERFACE_NAME: &str = "org.sol.Notifications1";

type WireAction = (String, String);
type WireRecord = (
    u64,
    String,
    String,
    String,
    String,
    Vec<WireAction>,
    String,
    u64,
);
type WireInvocation = (u64, String, String);

/// D-Bus interface backed by a typed notification daemon.
pub struct NotificationDbusService<S> {
    daemon: Arc<NotificationDaemon<S>>,
}

impl<S: NotificationStore> NotificationDbusService<S> {
    #[must_use]
    pub fn new(daemon: NotificationDaemon<S>) -> Self {
        Self {
            daemon: Arc::new(daemon),
        }
    }

    fn publish_request(
        &self,
        app_id: String,
        summary: String,
        body: String,
        urgency: String,
        actions: Vec<WireAction>,
        replaces: u64,
    ) -> zbus::fdo::Result<WireRecord> {
        let mut request =
            NotificationRequest::new(parse_app_id(&app_id)?, summary).map_err(fdo_error)?;
        if !body.is_empty() {
            request = request.with_body(body);
        }
        request = request.with_urgency(parse_urgency(&urgency)?);
        request = request
            .with_actions(actions_from_wire(actions)?)
            .map_err(fdo_error)?;
        if replaces != 0 {
            request = request.replacing(NotificationId::from_raw(replaces));
        }
        self.daemon
            .publish(request)
            .map(record_to_wire)
            .map_err(fdo_error)
    }

    fn dismiss_id(&self, id: u64, reason: String) -> zbus::fdo::Result<WireRecord> {
        self.daemon
            .dismiss(NotificationId::from_raw(id), parse_dismiss_reason(&reason)?)
            .map(record_to_wire)
            .map_err(fdo_error)
    }

    fn query_records(
        &self,
        scope: String,
        app_id: String,
        include_dismissed: bool,
    ) -> zbus::fdo::Result<Vec<WireRecord>> {
        let query = match scope.as_str() {
            "active" => NotificationQuery::Active,
            "history" => NotificationQuery::History,
            "app" => NotificationQuery::ForApp {
                app_id: parse_app_id(&app_id)?,
                include_dismissed,
            },
            _ => return Err(invalid_args("query scope must be active, history, or app")),
        };
        self.daemon
            .query(query)
            .map(|records| records.into_iter().map(record_to_wire).collect())
            .map_err(fdo_error)
    }

    fn invoke(&self, id: u64, action_id: String) -> zbus::fdo::Result<WireInvocation> {
        self.daemon
            .invoke_action(
                NotificationId::from_raw(id),
                NotificationActionId::new(action_id).map_err(fdo_error)?,
            )
            .map(invocation_to_wire)
            .map_err(fdo_error)
    }
}

#[zbus::interface(name = "org.sol.Notifications1")]
impl<S: NotificationStore + 'static> NotificationDbusService<S> {
    fn publish(
        &self,
        app_id: String,
        summary: String,
        body: String,
        urgency: String,
        actions: Vec<WireAction>,
        replaces: u64,
    ) -> zbus::fdo::Result<WireRecord> {
        self.publish_request(app_id, summary, body, urgency, actions, replaces)
    }

    fn dismiss(&self, id: u64, reason: String) -> zbus::fdo::Result<WireRecord> {
        self.dismiss_id(id, reason)
    }

    fn query(
        &self,
        scope: String,
        app_id: String,
        include_dismissed: bool,
    ) -> zbus::fdo::Result<Vec<WireRecord>> {
        self.query_records(scope, app_id, include_dismissed)
    }

    fn invoke_action(&self, id: u64, action_id: String) -> zbus::fdo::Result<WireInvocation> {
        self.invoke(id, action_id)
    }
}

/// Own the stable notification-service name on the caller's session bus.
pub fn serve_session<S: NotificationStore + 'static>(
    daemon: NotificationDaemon<S>,
) -> NotificationResult<Connection> {
    Builder::session()
        .map_err(bus_error)?
        .name(SERVICE_NAME)
        .map_err(bus_error)?
        .serve_at(OBJECT_PATH, NotificationDbusService::new(daemon))
        .map_err(bus_error)?
        .build()
        .map_err(bus_error)
}

/// A blocking SOL notification client for another process in the same session.
pub struct NotificationDbusProxy {
    proxy: Proxy<'static>,
}

impl std::fmt::Debug for NotificationDbusProxy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NotificationDbusProxy")
            .finish_non_exhaustive()
    }
}

impl NotificationDbusProxy {
    pub fn connect() -> NotificationResult<Self> {
        Self::from_connection(Connection::session().map_err(bus_error)?)
    }

    pub fn from_connection(connection: Connection) -> NotificationResult<Self> {
        Proxy::new_owned(connection, SERVICE_NAME, OBJECT_PATH, INTERFACE_NAME)
            .map(|proxy| Self { proxy })
            .map_err(bus_error)
    }

    fn call<A, R>(&self, method: &str, arguments: &A) -> NotificationResult<R>
    where
        A: serde::ser::Serialize + zbus::zvariant::Type,
        R: serde::de::DeserializeOwned + zbus::zvariant::Type,
    {
        self.proxy.call(method, arguments).map_err(bus_error)
    }
}

impl NotificationApi for NotificationDbusProxy {
    fn publish(&self, request: NotificationRequest) -> NotificationResult<NotificationRecord> {
        let record: WireRecord = self.call(
            "Publish",
            &(
                request.app_id.as_str(),
                request.summary,
                request.body.unwrap_or_default(),
                urgency_name(request.urgency),
                actions_to_wire(request.actions),
                request.replaces.map_or(0, NotificationId::get),
            ),
        )?;
        record_from_wire(record)
    }

    fn dismiss(
        &self,
        id: NotificationId,
        reason: NotificationDismissReason,
    ) -> NotificationResult<NotificationRecord> {
        let record: WireRecord = self.call("Dismiss", &(id.get(), dismiss_reason_name(reason)))?;
        record_from_wire(record)
    }

    fn query(&self, query: NotificationQuery) -> NotificationResult<Vec<NotificationRecord>> {
        let (scope, app_id, include_dismissed) = match query {
            NotificationQuery::Active => ("active", String::new(), false),
            NotificationQuery::History => ("history", String::new(), true),
            NotificationQuery::ForApp {
                app_id,
                include_dismissed,
            } => ("app", app_id.as_str().to_owned(), include_dismissed),
        };
        let records: Vec<WireRecord> = self.call("Query", &(scope, app_id, include_dismissed))?;
        records.into_iter().map(record_from_wire).collect()
    }

    fn invoke_action(
        &self,
        id: NotificationId,
        action_id: NotificationActionId,
    ) -> NotificationResult<NotificationActionInvocation> {
        let invocation: WireInvocation =
            self.call("InvokeAction", &(id.get(), action_id.as_str()))?;
        invocation_from_wire(invocation)
    }
}

fn record_to_wire(record: NotificationRecord) -> WireRecord {
    (
        record.id.get(),
        record.notification.app_id.as_str().to_owned(),
        record.notification.summary,
        record.notification.body.unwrap_or_default(),
        urgency_name(record.notification.urgency).to_owned(),
        actions_to_wire(record.notification.actions),
        lifecycle_name(record.lifecycle).to_owned(),
        record.sequence,
    )
}

fn record_from_wire(record: WireRecord) -> NotificationResult<NotificationRecord> {
    let (id, app_id, summary, body, urgency, actions, lifecycle, sequence) = record;
    let mut request = NotificationRequest::new(parse_app_id_notification(&app_id)?, summary)?;
    if !body.is_empty() {
        request = request.with_body(body);
    }
    request = request.with_urgency(parse_urgency_notification(&urgency)?);
    request = request.with_actions(actions_from_wire_notification(actions)?)?;
    Ok(NotificationRecord {
        id: NotificationId::from_raw(id),
        notification: request,
        lifecycle: parse_lifecycle_notification(&lifecycle)?,
        sequence,
    })
}

fn invocation_to_wire(invocation: NotificationActionInvocation) -> WireInvocation {
    (
        invocation.notification_id.get(),
        invocation.app_id.as_str().to_owned(),
        invocation.action_id.as_str().to_owned(),
    )
}

fn invocation_from_wire(
    invocation: WireInvocation,
) -> NotificationResult<NotificationActionInvocation> {
    let (notification_id, app_id, action_id) = invocation;
    Ok(NotificationActionInvocation {
        notification_id: NotificationId::from_raw(notification_id),
        app_id: parse_app_id_notification(&app_id)?,
        action_id: NotificationActionId::new(action_id)?,
    })
}

fn actions_to_wire(actions: Vec<NotificationAction>) -> Vec<WireAction> {
    actions
        .into_iter()
        .map(|action| (action.id.as_str().to_owned(), action.label))
        .collect()
}

fn actions_from_wire(actions: Vec<WireAction>) -> zbus::fdo::Result<Vec<NotificationAction>> {
    actions
        .into_iter()
        .map(|(id, label)| {
            NotificationAction::new(NotificationActionId::new(id).map_err(fdo_error)?, label)
                .map_err(fdo_error)
        })
        .collect()
}

fn actions_from_wire_notification(
    actions: Vec<WireAction>,
) -> NotificationResult<Vec<NotificationAction>> {
    actions
        .into_iter()
        .map(|(id, label)| NotificationAction::new(NotificationActionId::new(id)?, label))
        .collect()
}

fn parse_app_id(value: &str) -> zbus::fdo::Result<AppId> {
    AppId::parse(value).map_err(|error| invalid_args(error.to_string()))
}

fn parse_app_id_notification(value: &str) -> NotificationResult<AppId> {
    AppId::parse(value).map_err(|error| NotificationError::backend(error.to_string()))
}

fn urgency_name(urgency: NotificationUrgency) -> &'static str {
    match urgency {
        NotificationUrgency::Low => "low",
        NotificationUrgency::Normal => "normal",
        NotificationUrgency::Critical => "critical",
    }
}

fn parse_urgency(value: &str) -> zbus::fdo::Result<NotificationUrgency> {
    parse_urgency_notification(value).map_err(fdo_error)
}

fn parse_urgency_notification(value: &str) -> NotificationResult<NotificationUrgency> {
    match value {
        "low" => Ok(NotificationUrgency::Low),
        "normal" => Ok(NotificationUrgency::Normal),
        "critical" => Ok(NotificationUrgency::Critical),
        _ => Err(NotificationError::InvalidRequest(
            "notification urgency must be low, normal, or critical",
        )),
    }
}

fn dismiss_reason_name(reason: NotificationDismissReason) -> &'static str {
    match reason {
        NotificationDismissReason::User => "user",
        NotificationDismissReason::Application => "application",
        NotificationDismissReason::Expired => "expired",
    }
}

fn parse_dismiss_reason(value: &str) -> zbus::fdo::Result<NotificationDismissReason> {
    match value {
        "user" => Ok(NotificationDismissReason::User),
        "application" => Ok(NotificationDismissReason::Application),
        "expired" => Ok(NotificationDismissReason::Expired),
        _ => Err(invalid_args(
            "dismiss reason must be user, application, or expired",
        )),
    }
}

fn lifecycle_name(lifecycle: NotificationLifecycle) -> &'static str {
    match lifecycle {
        NotificationLifecycle::Active => "active",
        NotificationLifecycle::Dismissed(reason) => match reason {
            NotificationDismissReason::User => "dismissed-user",
            NotificationDismissReason::Application => "dismissed-application",
            NotificationDismissReason::Expired => "dismissed-expired",
        },
    }
}

fn parse_lifecycle_notification(value: &str) -> NotificationResult<NotificationLifecycle> {
    match value {
        "active" => Ok(NotificationLifecycle::Active),
        "dismissed-user" => Ok(NotificationLifecycle::Dismissed(
            NotificationDismissReason::User,
        )),
        "dismissed-application" => Ok(NotificationLifecycle::Dismissed(
            NotificationDismissReason::Application,
        )),
        "dismissed-expired" => Ok(NotificationLifecycle::Dismissed(
            NotificationDismissReason::Expired,
        )),
        _ => Err(NotificationError::backend(
            "unknown notification lifecycle from D-Bus",
        )),
    }
}

fn invalid_args(message: impl Into<String>) -> zbus::fdo::Error {
    zbus::fdo::Error::InvalidArgs(message.into())
}
fn fdo_error(error: NotificationError) -> zbus::fdo::Error {
    zbus::fdo::Error::Failed(error.to_string())
}
fn bus_error(error: impl std::fmt::Display) -> NotificationError {
    NotificationError::backend(format!("notification D-Bus: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{record_from_wire, record_to_wire};
    use sol_system::{
        AppId, NotificationAction, NotificationActionId, NotificationLifecycle,
        NotificationRequest, NotificationUrgency,
    };

    #[test]
    fn notification_record_wire_round_trip_preserves_typed_values() {
        let mut request =
            NotificationRequest::new(AppId::parse("org.sol.files").unwrap(), "Copied").unwrap();
        request = request
            .with_body("report.pdf")
            .with_urgency(NotificationUrgency::Critical);
        request = request
            .with_actions(vec![
                NotificationAction::new(NotificationActionId::new("open").unwrap(), "Open")
                    .unwrap(),
            ])
            .unwrap();
        let record = sol_system::NotificationRecord {
            id: sol_system::NotificationId::from_raw(4),
            notification: request,
            lifecycle: NotificationLifecycle::Active,
            sequence: 9,
        };
        assert_eq!(record_from_wire(record_to_wire(record.clone())), Ok(record));
    }
}
