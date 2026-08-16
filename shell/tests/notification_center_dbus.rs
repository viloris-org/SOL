//! Runs only under `scripts/validate-notification-center-dbus.sh`, which owns
//! an isolated bus and a real `sol-notificationd --dbus` process.

use sol_notificationd::dbus::NotificationDbusProxy;
use sol_shell::notification_center::{
    NotificationCenter, NotificationCenterKey, NotificationCenterOutcome, NotificationScope,
};
use sol_system::{
    AppId, NotificationAction, NotificationActionId, NotificationActionInvocation, NotificationApi,
    NotificationDismissReason, NotificationLifecycle, NotificationRequest, NotificationUrgency,
};

#[test]
#[ignore = "requires the isolated service started by validate-notification-center-dbus.sh"]
fn center_drives_notificationd_through_its_dbus_proxy() {
    let proxy = NotificationDbusProxy::connect().expect("connect to notification D-Bus service");
    let app_id = AppId::parse("org.sol.files").expect("valid app ID");
    let action_id = NotificationActionId::new("open").expect("valid action ID");
    let action = NotificationAction::new(action_id.clone(), "Open").expect("valid action");
    let published = proxy
        .publish(
            NotificationRequest::new(app_id.clone(), "Copy complete")
                .expect("valid notification")
                .with_body("report-final.pdf")
                .with_urgency(NotificationUrgency::Critical)
                .with_actions(vec![action])
                .expect("valid actions"),
        )
        .expect("publish through notification D-Bus proxy");

    let mut center = NotificationCenter::new(proxy);
    center.refresh().expect("load active notifications");
    assert_eq!(center.records().len(), 1);
    let groups = center.groups();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].key.app_id, app_id);
    assert_eq!(groups[0].key.urgency, NotificationUrgency::Critical);

    assert_eq!(
        center.handle_key(NotificationCenterKey::ArrowDown),
        Ok(NotificationCenterOutcome::FocusChanged(published.id))
    );
    assert_eq!(
        center.handle_key(NotificationCenterKey::Enter),
        Ok(NotificationCenterOutcome::ActionInvoked(
            NotificationActionInvocation {
                notification_id: published.id,
                app_id: app_id.clone(),
                action_id,
            }
        ))
    );
    assert!(matches!(
        center.handle_key(NotificationCenterKey::Delete),
        Ok(NotificationCenterOutcome::Dismissed(record))
            if record.id == published.id
                && record.lifecycle
                    == NotificationLifecycle::Dismissed(NotificationDismissReason::User)
    ));
    assert!(center.records().is_empty());

    center
        .set_scope(NotificationScope::History)
        .expect("load retained notification history");
    assert_eq!(center.records().len(), 1);
    assert_eq!(center.records()[0].id, published.id);
    assert_eq!(
        center.records()[0].lifecycle,
        NotificationLifecycle::Dismissed(NotificationDismissReason::User)
    );
}
