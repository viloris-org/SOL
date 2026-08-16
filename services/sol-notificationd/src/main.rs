use std::{
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use sol_notificationd::{MemoryNotificationStore, NotificationDaemon, dbus};

fn main() -> ExitCode {
    let daemon = match NotificationDaemon::new(MemoryNotificationStore::new()) {
        Ok(daemon) => daemon,
        Err(error) => {
            eprintln!("sol-notificationd: failed to initialize: {error}");
            return ExitCode::from(1);
        }
    };
    if !std::env::args().any(|argument| argument == "--dbus") {
        println!("sol-notificationd: typed notification service ready");
        return ExitCode::SUCCESS;
    }
    let _connection = match dbus::serve_session(daemon) {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("sol-notificationd: failed to own D-Bus service: {error}");
            return ExitCode::from(1);
        }
    };
    println!("sol-notificationd: D-Bus service ready");
    let running = Arc::new(AtomicBool::new(true));
    if let Err(error) = ctrlc::set_handler({
        let running = Arc::clone(&running);
        move || running.store(false, Ordering::SeqCst)
    }) {
        eprintln!("sol-notificationd: cannot install shutdown handler: {error}");
        return ExitCode::from(1);
    }
    while running.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    ExitCode::SUCCESS
}
