//! Runs only under `scripts/validate-notificationd-dbus.sh`, which owns an
//! isolated bus and a real `sol-notificationd --dbus` process.

use sol_notificationd::dbus::NotificationDbusProxy;
use sol_system::{
    AppId, NotificationAction, NotificationActionId, NotificationApi, NotificationDismissReason,
    NotificationLifecycle, NotificationQuery, NotificationRequest, NotificationUrgency,
};

#[test]
#[ignore = "requires the isolated service started by validate-notificationd-dbus.sh"]
fn proxy_drives_the_real_notification_service() {
    let proxy = NotificationDbusProxy::connect().expect("connect to notification D-Bus service");
    let app_id = AppId::parse("org.sol.files").expect("valid app ID");
    let action = NotificationAction::new(
        NotificationActionId::new("open").expect("valid action ID"),
        "Open",
    )
    .expect("valid action");
    let request = NotificationRequest::new(app_id.clone(), "Copy complete")
        .expect("valid notification")
        .with_body("report.pdf")
        .with_urgency(NotificationUrgency::Critical)
        .with_actions(vec![action])
        .expect("actions are valid");

    let published = proxy.publish(request).expect("publish notification");
    assert_eq!(published.sequence, 1);
    assert_eq!(
        published.notification.urgency,
        NotificationUrgency::Critical
    );

    let replacement = proxy
        .publish(
            NotificationRequest::new(app_id.clone(), "Copy complete")
                .expect("valid replacement")
                .with_body("report-final.pdf")
                .with_actions(vec![
                    NotificationAction::new(
                        NotificationActionId::new("open").expect("valid action ID"),
                        "Open",
                    )
                    .expect("valid action"),
                ])
                .expect("valid actions")
                .replacing(published.id),
        )
        .expect("replace notification");
    assert_eq!(replacement.id, published.id);
    assert_eq!(replacement.sequence, 2);

    let invocation = proxy
        .invoke_action(
            replacement.id,
            NotificationActionId::new("open").expect("valid action ID"),
        )
        .expect("validate action invocation");
    assert_eq!(invocation.app_id, app_id);

    let dismissed = proxy
        .dismiss(replacement.id, NotificationDismissReason::User)
        .expect("dismiss notification");
    assert_eq!(
        dismissed.lifecycle,
        NotificationLifecycle::Dismissed(NotificationDismissReason::User)
    );
    assert!(
        proxy
            .query(NotificationQuery::Active)
            .expect("query active notifications")
            .is_empty()
    );
    assert_eq!(
        proxy
            .query(NotificationQuery::History)
            .expect("query notification history"),
        vec![dismissed]
    );
}
