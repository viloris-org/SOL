use std::{
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use sol_settingsd::{FileSettingsStore, SettingsDaemon, dbus};

fn main() -> ExitCode {
    let path = settings_path();
    let daemon = match SettingsDaemon::new(FileSettingsStore::new(&path)) {
        Ok(daemon) => daemon,
        Err(error) => {
            eprintln!("sol-settingsd: failed to initialize settings store: {error}");
            return ExitCode::from(1);
        }
    };

    if !std::env::args().any(|argument| argument == "--dbus") {
        println!("sol-settingsd: settings store ready at {}", path.display());
        return ExitCode::SUCCESS;
    }

    let _connection = match dbus::serve_session(daemon) {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("sol-settingsd: failed to own D-Bus service: {error}");
            return ExitCode::from(1);
        }
    };
    println!("sol-settingsd: D-Bus service ready at {}", path.display());

    let running = Arc::new(AtomicBool::new(true));
    if let Err(error) = ctrlc::set_handler({
        let running = Arc::clone(&running);
        move || running.store(false, Ordering::SeqCst)
    }) {
        eprintln!("sol-settingsd: cannot install shutdown handler: {error}");
        return ExitCode::from(1);
    }
    while running.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    ExitCode::SUCCESS
}

fn settings_path() -> std::path::PathBuf {
    if let Some(path) = std::env::var_os("SOL_SETTINGS_PATH") {
        return std::path::PathBuf::from(path);
    }
    if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
        return std::path::PathBuf::from(config_home).join("sol/settings.conf");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return std::path::PathBuf::from(home).join(".config/sol/settings.conf");
    }
    std::path::PathBuf::from("sol/settings.conf")
}
