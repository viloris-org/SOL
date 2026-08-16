use sol_diagnostics::{
    DEFAULT_RETENTION, DiagnosticCode, DiagnosticSeverity, DiagnosticSource, DiagnosticStore,
    FileDiagnosticStore, SolComponent, install_panic_capture,
};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const CHILD_PATH: &str = "SOL_DIAGNOSTICS_PANIC_CAPTURE_TEST_PATH";

#[test]
fn real_child_panic_is_persisted_as_a_bounded_typed_record() {
    if let Some(path) = std::env::var_os(CHILD_PATH) {
        install_panic_capture(
            DiagnosticSource::Component(SolComponent::Shell),
            PathBuf::from(path),
            DEFAULT_RETENTION,
        )
        .expect("install child panic capture");
        panic!("startup failed token=super-secret at /home/alice/private.txt");
    }

    let directory = temporary_directory();
    fs::create_dir_all(&directory).expect("create panic capture fixture directory");
    let path = directory.join("diagnostics.log");
    let output = Command::new(std::env::current_exe().expect("locate test executable"))
        .args([
            "--exact",
            "real_child_panic_is_persisted_as_a_bounded_typed_record",
            "--nocapture",
        ])
        .env(CHILD_PATH, &path)
        .output()
        .expect("run crashing child process");
    assert!(
        !output.status.success(),
        "child process unexpectedly succeeded"
    );

    let snapshot = FileDiagnosticStore::new(&path)
        .load()
        .expect("load captured panic")
        .expect("panic capture file should exist");
    assert_eq!(snapshot.records.len(), 1);
    let record = &snapshot.records[0];
    assert_eq!(
        record.event.source,
        DiagnosticSource::Component(SolComponent::Shell)
    );
    assert_eq!(record.event.severity, DiagnosticSeverity::Fatal);
    assert_eq!(record.event.code, DiagnosticCode::ProcessCrash);
    let message = record
        .event
        .message()
        .expect("panic record should contain a bounded summary")
        .as_str();
    assert!(message.contains("panic at"));
    assert!(message.contains("token=[redacted]"));
    assert!(message.contains("[redacted-path]"));
    assert!(!message.contains("super-secret"));
    assert!(!message.contains("alice"));

    fs::remove_dir_all(&directory).expect("remove panic capture fixture directory");
}

fn temporary_directory() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("fixture clock should follow Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "sol-diagnostics-panic-{}-{nonce}",
        std::process::id()
    ))
}
