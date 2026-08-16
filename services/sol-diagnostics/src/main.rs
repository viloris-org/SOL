use sol_diagnostics::{
    DEFAULT_RETENTION, DiagnosticsService, FileDiagnosticStore, default_diagnostics_path,
};

fn main() {
    let path = default_diagnostics_path();
    match DiagnosticsService::new(FileDiagnosticStore::new(&path), DEFAULT_RETENTION) {
        Ok(_) => println!(
            "sol-diagnostics: typed, redacted local store ready at {}",
            path.display()
        ),
        Err(error) => eprintln!("sol-diagnostics: failed to initialize: {error}"),
    }
}
