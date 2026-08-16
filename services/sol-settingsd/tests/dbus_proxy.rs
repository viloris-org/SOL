//! Runs only under `scripts/validate-settingsd-dbus.sh`, which owns an
//! isolated bus and a real `sol-settingsd --dbus` process.

use sol_settingsd::dbus::SettingsDbusProxy;
use sol_system::{SettingsApi, SettingsChange};

#[test]
#[ignore = "requires the isolated service started by validate-settingsd-dbus.sh"]
fn proxy_reads_and_mutates_the_real_session_bus_service() {
    let proxy = SettingsDbusProxy::connect().expect("connect to the settings D-Bus service");
    let initial = proxy.snapshot().expect("read initial settings snapshot");
    assert_eq!(initial.revision, 0);
    assert!(!initial.appearance.high_contrast);

    let updated = proxy
        .apply(SettingsChange::SetHighContrast(true))
        .expect("apply typed setting through the D-Bus proxy");
    assert_eq!(updated.revision, 1);
    assert!(updated.appearance.high_contrast);
}
