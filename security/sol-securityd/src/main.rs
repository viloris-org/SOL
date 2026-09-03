use sol_securityd::{SecurityConfig, SecurityService, serve};
use std::{
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

fn main() -> ExitCode {
    if let Ok(filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    } else {
        tracing_subscriber::fmt().init();
    }
    let service = match SecurityService::open(SecurityConfig::system_default()) {
        Ok(service) => Arc::new(service),
        Err(error) => {
            tracing::error!(%error, "cannot initialize sol-securityd");
            return ExitCode::FAILURE;
        }
    };
    let shutdown = Arc::new(AtomicBool::new(false));
    if let Err(error) = ctrlc::set_handler({
        let shutdown = Arc::clone(&shutdown);
        move || shutdown.store(true, Ordering::Release)
    }) {
        tracing::error!(%error, "cannot install shutdown handler");
        return ExitCode::FAILURE;
    }
    match serve(&service, &shutdown) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "sol-securityd stopped");
            ExitCode::FAILURE
        }
    }
}
