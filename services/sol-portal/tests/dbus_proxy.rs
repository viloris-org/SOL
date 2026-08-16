//! Runs only under `scripts/validate-portal-dbus.sh`, which owns an isolated
//! bus and a real default-deny `sol-portal --dbus` process.

use sol_portal::{
    PortalRequest,
    dbus::{PortalDbusOutcome, PortalDbusProxy},
};
use sol_system::AppId;

#[test]
#[ignore = "requires the isolated service started by validate-portal-dbus.sh"]
fn proxy_preserves_default_deny_for_sensitive_requests() {
    let proxy = PortalDbusProxy::connect().expect("connect to portal D-Bus service");
    let caller = AppId::parse("org.sol.files").expect("valid caller ID");

    assert_eq!(
        proxy
            .request(
                &caller,
                &PortalRequest::OpenDocument {
                    uri: "file:///tmp/report.txt".to_owned(),
                },
            )
            .expect("evaluate document request"),
        PortalDbusOutcome::Denied
    );
    assert_eq!(
        proxy
            .request(&caller, &PortalRequest::ScreenCapture)
            .expect("evaluate capture request"),
        PortalDbusOutcome::Denied
    );
}
