//! A process must not be able to talk its way into a privileged identity.
//!
//! `/proc/<pid>/comm` is a label a process writes for itself, so it can name
//! itself `sol-logind` for the cost of one `write`. It used to be the whole of
//! the compositor's identity check, which made the session lock — a full-screen
//! surface with exclusive input, i.e. a convincing password prompt — reachable by
//! anything running as the user.
//!
//! This lives in its own test binary because it renames the process, which is
//! process-wide state no other test should have to reason about.

// `expect` in a test is a deliberate assertion, not an unhandled error.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serial_test::serial;
use sol_compositor::scp::protocol::{ClientMessage, CompositorMessage};
use sol_compositor::scp::state::ScpState;

/// Rename this process the way any unprivileged process may rename itself.
///
/// `prctl(PR_SET_NAME)` renames the calling *thread*; the compositor reads the
/// thread-group leader, which is what a real attacker would target too.
fn rename_process(name: &str) {
    let pid = std::process::id();
    std::fs::write(format!("/proc/self/task/{pid}/comm"), name)
        .expect("a process may always rename its own main thread");
}

fn current_comm() -> String {
    let pid = std::process::id();
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .expect("read own comm")
        .trim()
        .to_string()
}

#[test]
#[serial]
fn renaming_the_process_does_not_grant_the_session_lock() {
    rename_process("sol-logind");
    assert_eq!(
        current_comm(),
        "sol-logind",
        "the rename must actually have taken effect, or this test proves nothing"
    );

    let mut state = ScpState::new();
    let replies = state
        .handle_message(
            None,
            ClientMessage::Connect {
                app_id: "sol-logind".to_string(),
                pid: std::process::id(),
            },
        )
        .expect_err("a reserved name from an untrusted binary must not connect");

    assert!(
        replies.contains("verify app identity"),
        "unexpected rejection: {replies}"
    );
}

#[test]
#[serial]
fn renaming_the_process_does_not_grant_layer_shell() {
    rename_process("sol-shell");

    let mut state = ScpState::new();
    let result = state.handle_message(
        None,
        ClientMessage::Connect {
            app_id: "sol-shell".to_string(),
            pid: std::process::id(),
        },
    );

    assert!(
        result.is_err(),
        "the shell identity gates layer shell and must not be self-assignable: {result:?}"
    );
}

#[test]
#[serial]
fn an_ordinary_name_still_connects() {
    // The hardening must not turn into "nothing may connect": an app with an
    // unreserved name is exactly what the compositor exists to serve.
    rename_process("sol-files");

    let mut state = ScpState::new();
    let replies = state
        .handle_message(
            None,
            ClientMessage::Connect {
                app_id: "sol-files".to_string(),
                pid: std::process::id(),
            },
        )
        .expect("an ordinary application connects");

    assert!(
        matches!(replies.first(), Some(CompositorMessage::Connected { .. })),
        "unexpected reply: {replies:?}"
    );
}
