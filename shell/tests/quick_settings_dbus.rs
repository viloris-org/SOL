//! Runs only under `scripts/validate-quick-settings-dbus.sh`, which owns an
//! isolated bus and a real `sol-settingsd --dbus` process.

use sol_settingsd::dbus::SettingsDbusProxy;
use sol_shell::quick_settings::{QuickSettings, QuickSettingsOutcome};
use sol_system::{
    AppId, ColorScheme, DefaultDenyPolicy, MemoryActionAuditStore, MemoryPermissionStore,
    OutputVolume, PermissionGrant, PermissionKey, PermissionStore, SettingsApi,
    SystemActionService, SystemCapability,
};

#[test]
#[ignore = "requires the isolated service started by validate-quick-settings-dbus.sh"]
fn quick_settings_drives_the_real_settings_service() {
    let permissions = MemoryPermissionStore::default();
    permissions
        .set(
            PermissionKey::new(
                AppId::parse("org.sol.shell").expect("valid shell app ID"),
                SystemCapability::ChangeQuickSettings,
            ),
            PermissionGrant::Allow,
        )
        .expect("grant quick settings capability");
    let actions = SystemActionService::new(
        DefaultDenyPolicy,
        permissions,
        MemoryActionAuditStore::default(),
    );
    let mut quick = QuickSettings::new(
        SettingsDbusProxy::connect().expect("connect to settings daemon"),
        actions,
    )
    .expect("construct Quick Settings from daemon snapshot");

    assert!(matches!(
        quick
            .set_color_scheme(ColorScheme::Dark)
            .expect("apply appearance setting"),
        QuickSettingsOutcome::Applied(_)
    ));
    assert!(matches!(
        quick
            .set_volume(OutputVolume::new(67).expect("valid volume"))
            .expect("authorize and apply volume"),
        QuickSettingsOutcome::Applied(_)
    ));
    assert!(matches!(
        quick.set_muted(true).expect("authorize and apply mute"),
        QuickSettingsOutcome::Applied(_)
    ));

    let persisted = SettingsDbusProxy::connect()
        .expect("reconnect to settings daemon")
        .snapshot()
        .expect("read persisted daemon snapshot");
    assert_eq!(persisted.revision, 3);
    assert_eq!(persisted.appearance.color_scheme, ColorScheme::Dark);
    assert_eq!(persisted.audio.output_volume.percent(), 67);
    assert!(persisted.audio.output_muted);
}
