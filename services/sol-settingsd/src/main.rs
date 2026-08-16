use sol_settingsd::{FileSettingsStore, SettingsDaemon};

fn main() {
    let path = settings_path();
    match SettingsDaemon::new(FileSettingsStore::new(&path)) {
        Ok(_) => println!("sol-settingsd: settings store ready at {}", path.display()),
        Err(error) => eprintln!("sol-settingsd: failed to initialize settings store: {error}"),
    }
}

fn settings_path() -> std::path::PathBuf {
    if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
        return std::path::PathBuf::from(config_home).join("sol/settings.conf");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return std::path::PathBuf::from(home).join(".config/sol/settings.conf");
    }
    std::path::PathBuf::from("sol/settings.conf")
}
