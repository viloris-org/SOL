use std::{
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use sol_portal::{PortalService, dbus};
use sol_system::{
    DefaultDenyPolicy, MemoryActionAuditStore, MemoryPermissionStore, SystemActionService,
};

fn main() -> ExitCode {
    let portal = PortalService::new(SystemActionService::new(
        DefaultDenyPolicy,
        MemoryPermissionStore::default(),
        MemoryActionAuditStore::default(),
    ));
    if !std::env::args().any(|argument| argument == "--dbus") {
        println!("sol-portal: typed permission-bound request service ready");
        return ExitCode::SUCCESS;
    }
    let _connection = match dbus::serve_session(portal) {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("sol-portal: failed to own D-Bus service: {error}");
            return ExitCode::from(1);
        }
    };
    println!("sol-portal: D-Bus authorization service ready");
    let running = Arc::new(AtomicBool::new(true));
    if let Err(error) = ctrlc::set_handler({
        let running = Arc::clone(&running);
        move || running.store(false, Ordering::SeqCst)
    }) {
        eprintln!("sol-portal: cannot install shutdown handler: {error}");
        return ExitCode::from(1);
    }
    while running.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    ExitCode::SUCCESS
}
