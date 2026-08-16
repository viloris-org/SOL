use sol_diagnostics::{DEFAULT_RETENTION, DiagnosticsService, FileDiagnosticStore};
use std::path::PathBuf;

fn main() {
    let path = diagnostics_path();
    match DiagnosticsService::new(FileDiagnosticStore::new(&path), DEFAULT_RETENTION) {
        Ok(_) => println!(
            "sol-diagnostics: typed, redacted local store ready at {}",
            path.display()
        ),
        Err(error) => eprintln!("sol-diagnostics: failed to initialize: {error}"),
    }
}

fn diagnostics_path() -> PathBuf {
    if let Some(state_home) = std::env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(state_home).join("sol/diagnostics.log");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".local/state/sol/diagnostics.log");
    }
    PathBuf::from("sol/diagnostics.log")
}
