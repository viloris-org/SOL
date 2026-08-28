//! The login screen's session-lock handshake, against a real compositor state.
//!
//! [`LockDriver`] is driven message-for-message through
//! [`ScpState`](sol_compositor::scp::ScpState) rather than a mock, so the
//! sequence it produces is checked against the compositor that will actually
//! answer it. The transport is left out for the same reason
//! `compositor/tests/scp_input.rs` leaves it out: what is under test is the
//! protocol exchange, not how the bytes are framed.
//!
//! Each client registers a [`SessionSink`] exactly as the transport does, so
//! events the compositor pushes — input, frame callbacks — are observed the way
//! a real greeter would receive them.

use std::{collections::HashMap, sync::Arc, sync::Mutex};

use sol_compositor::scp::{
    ScpState,
    capability::{Capability, CapabilityToken, Decision},
    event_queue::SessionSink,
    protocol::{ClientMessage, CompositorMessage, KeyState, SessionId},
    security::{AppId, AuditOutcome, SecurityCoordinator},
};
use sol_logind::{
    KeyInput, LoginUi, Modifiers, UserAccount,
    scp::{
        keys,
        lock::{LockDriver, LockError, LockEvent, LockPhase},
    },
};

const OUTPUT_WIDTH: i32 = 1920;
const OUTPUT_HEIGHT: i32 = 1080;

/// The identity the compositor reserves the session lock for.
const GREETER: &str = "sol-logind";

/// A coordinator that grants what it is asked for.
///
/// Real policy — which binary may claim `sol-logind`, and therefore who may
/// cover the screen — lives in `sol_compositor::scp::security` and is tested
/// there. Repeating it here would only mean this test could never run, because a
/// test binary is not the installed greeter.
#[derive(Default)]
struct PermissiveSecurity {
    tokens: Mutex<HashMap<Vec<u8>, (AppId, Capability)>>,
}

impl SecurityCoordinator for PermissiveSecurity {
    fn verify_app_identity(&self, _pid: u32) -> Option<AppId> {
        Some(AppId(GREETER.to_string()))
    }

    fn evaluate_capability(&self, app_id: &AppId, cap: &Capability) -> Decision {
        Decision::Granted {
            token: self.issue_token(app_id, cap),
            expires_at: None,
        }
    }

    fn issue_token(&self, app_id: &AppId, cap: &Capability) -> CapabilityToken {
        let data = format!("{}:{}", app_id.0, cap.wire_name()).into_bytes();
        self.tokens
            .lock()
            .expect("token registry")
            .insert(data.clone(), (app_id.clone(), cap.clone()));
        CapabilityToken {
            data,
            expires_at: None,
            one_time: false,
        }
    }

    fn verify_token(&self, token: &CapabilityToken) -> Option<(AppId, Capability)> {
        self.tokens
            .lock()
            .expect("token registry")
            .get(&token.data)
            .cloned()
    }

    fn audit_capability_use(&self, _app_id: &AppId, _cap: &Capability, _outcome: AuditOutcome) {}
}

/// A compositor with one output registered, as a real machine would have.
fn compositor() -> ScpState {
    let mut state = ScpState::with_security(Arc::new(PermissiveSecurity::default()));
    state.output_manager_mut().add_output(
        "TEST-1".to_string(),
        "test output".to_string(),
        OUTPUT_WIDTH,
        OUTPUT_HEIGHT,
        60_000,
    );
    state
}

/// A greeter connected to `state`, pumping the driver until it goes quiet.
struct Greeter {
    driver: LockDriver,
    session_id: Option<SessionId>,
    sink: Arc<SessionSink>,
    events: Vec<LockEvent>,
}

impl Greeter {
    fn connect(state: &mut ScpState) -> Result<Self, LockError> {
        Self::connect_as(state, GREETER)
    }

    /// Connect claiming `app_id`, so a test can play something other than the
    /// greeter.
    fn connect_as(state: &mut ScpState, app_id: &str) -> Result<Self, LockError> {
        let mut greeter = Self {
            driver: LockDriver::new(),
            session_id: None,
            sink: SessionSink::new().expect("create an outbound event queue"),
            events: Vec::new(),
        };
        let connect = greeter.driver.start(app_id.to_string(), std::process::id());
        greeter.exchange(state, connect)?;
        Ok(greeter)
    }

    /// Send one request, then keep feeding replies back until nothing is left.
    ///
    /// This is what the transport does: a reply can require another request
    /// (`Connected` → `RequestCapability` → `LockSession` → …), so the exchange
    /// runs to a fixed point rather than one message at a time.
    fn exchange(&mut self, state: &mut ScpState, message: ClientMessage) -> Result<(), LockError> {
        let mut queue = vec![message];
        while let Some(message) = queue.pop() {
            let responses = state
                .handle_message(self.session_id, message)
                .unwrap_or_else(|error| panic!("the compositor refused a request: {error}"));

            for response in responses {
                if let CompositorMessage::Connected { session_id, .. } = &response {
                    self.session_id = Some(*session_id);
                    // Register the outbound queue exactly as the transport does,
                    // or compositor-pushed events would be dropped.
                    state.register_session_sink(*session_id, Arc::clone(&self.sink));
                }
                let step = self.driver.handle(response)?;
                self.events.extend(step.events);
                queue.extend(step.outbound.into_iter().rev());
            }
        }
        self.drain_pushed_events()
    }

    /// Feed the driver everything the compositor queued for this client.
    fn drain_pushed_events(&mut self) -> Result<(), LockError> {
        for event in self.sink.drain() {
            let step = self.driver.handle(event.message)?;
            self.events.extend(step.events);
            assert!(
                step.outbound.is_empty(),
                "a pushed event should not need a reply here"
            );
        }
        Ok(())
    }

    fn take_events(&mut self) -> Vec<LockEvent> {
        std::mem::take(&mut self.events)
    }
}

fn users() -> Vec<UserAccount> {
    vec![
        UserAccount::new("jdoe".into(), "John Doe".into(), 1000),
        UserAccount::new("asmith".into(), "Ann Smith".into(), 1001),
    ]
}

#[test]
fn the_greeter_locks_the_session_end_to_end() {
    let mut state = compositor();
    let mut greeter = Greeter::connect(&mut state).expect("the handshake completes");

    assert_eq!(greeter.driver.phase(), &LockPhase::Locked);
    assert_eq!(
        greeter.driver.size(),
        (OUTPUT_WIDTH, OUTPUT_HEIGHT),
        "a lock surface always covers its whole output"
    );

    let events = greeter.take_events();
    assert!(
        events.contains(&LockEvent::Resized {
            width: OUTPUT_WIDTH,
            height: OUTPUT_HEIGHT,
        }),
        "the greeter is told what size to draw: {events:?}"
    );
    assert!(
        events.contains(&LockEvent::Locked),
        "the greeter is told the session is locked: {events:?}"
    );

    // The compositor agrees, from its own side of the connection.
    let lock = state.session_lock().lock().expect("a lock is engaged");
    assert!(lock.is_confirmed(), "every output must be covered");
    assert_eq!(lock.app_id, AppId(GREETER.to_string()));
}

#[test]
fn locking_hands_the_greeter_exclusive_keyboard_focus() {
    let mut state = compositor();
    let mut greeter = Greeter::connect(&mut state).expect("the handshake completes");
    greeter.take_events();

    // Keystrokes reach the lock surface, and only it.
    state.handle_key(38, KeyState::Pressed, 0); // 'a'
    state.handle_key(38, KeyState::Released, 0);
    greeter
        .drain_pushed_events()
        .expect("input events are understood");

    let keys: Vec<_> = greeter
        .take_events()
        .into_iter()
        .filter(|event| matches!(event, LockEvent::Key { .. }))
        .collect();
    assert_eq!(
        keys,
        vec![
            LockEvent::Key {
                keycode: 38,
                pressed: true,
            },
            LockEvent::Key {
                keycode: 38,
                pressed: false,
            },
        ]
    );
}

#[test]
fn typed_keys_become_a_password_in_the_login_state() {
    let mut state = compositor();
    let mut greeter = Greeter::connect(&mut state).expect("the handshake completes");
    greeter.take_events();

    // "Hi" — shift is held for the capital, exactly as a keyboard reports it.
    for (keycode, press) in [
        (50, true), // Left Shift down
        (43, true), // h
        (43, false),
        (50, false), // Left Shift up
        (31, true),  // i
        (31, false),
    ] {
        state.handle_key(
            keycode,
            if press {
                KeyState::Pressed
            } else {
                KeyState::Released
            },
            0,
        );
    }
    greeter.drain_pushed_events().expect("input is understood");

    // Replay the events through the same decode path the login loop uses.
    let mut ui = LoginUi::new(users());
    let mut modifiers = Modifiers::default();
    for event in greeter.take_events() {
        match event {
            LockEvent::Modifiers {
                depressed,
                latched,
                locked,
            } => modifiers = Modifiers::from_masks(depressed, latched, locked),
            LockEvent::Key {
                keycode,
                pressed: true,
            } => match keys::decode(keycode, modifiers) {
                Some(KeyInput::Char(character)) => ui.push_password_char(character),
                Some(KeyInput::Backspace) => ui.backspace(),
                _ => {}
            },
            _ => {}
        }
    }

    assert_eq!(ui.password, "Hi");
    assert!(ui.can_login());
}

#[test]
fn a_rendered_frame_is_accepted_as_a_lock_surface_buffer() {
    let mut state = compositor();
    let mut greeter = Greeter::connect(&mut state).expect("the handshake completes");

    // A real buffer: the compositor checks the descriptor's size and seals, so a
    // placeholder would not get past `validate_buffer`.
    let buffer = sol_logind::FrameBuffer::new(OUTPUT_WIDTH, OUTPUT_HEIGHT)
        .expect("allocate a lock-surface buffer");
    let presentation = greeter
        .driver
        .present(
            buffer.as_raw_fd(),
            buffer.width(),
            buffer.height(),
            buffer.stride(),
        )
        .expect("present a frame");

    let mut messages = vec![presentation.attach];
    messages.extend(presentation.rest);
    for message in messages {
        state
            .handle_message(greeter.session_id, message)
            .unwrap_or_else(|error| panic!("the compositor refused a frame: {error}"));
    }

    // The frame callback comes back, which is the compositor asking for the next
    // frame — the loop the login screen runs on.
    assert!(state.send_frame_callbacks(16) > 0);
    greeter
        .drain_pushed_events()
        .expect("the frame callback is understood");
    assert!(
        greeter.take_events().contains(&LockEvent::Frame),
        "the greeter must learn when it may draw again"
    );
}

#[test]
fn unlocking_releases_the_screen_and_relocking_takes_it_back() {
    let mut state = compositor();
    let mut greeter = Greeter::connect(&mut state).expect("the handshake completes");
    greeter.take_events();

    for message in greeter.driver.unlock().expect("unlock") {
        greeter.exchange(&mut state, message).expect("unlock lands");
    }
    assert!(
        state.session_lock().lock().is_none(),
        "a released lock leaves nothing behind"
    );

    // The user's session ended; take the screen back without re-asking for the
    // capability.
    let relock = greeter.driver.relock().expect("relock");
    greeter.exchange(&mut state, relock).expect("relock lands");

    assert_eq!(greeter.driver.phase(), &LockPhase::Locked);
    assert!(
        state
            .session_lock()
            .lock()
            .expect("locked again")
            .is_confirmed()
    );
}

#[test]
fn a_crashed_greeter_leaves_the_session_locked() {
    let mut state = compositor();
    let greeter = Greeter::connect(&mut state).expect("the handshake completes");
    let session_id = greeter.session_id.expect("connected");

    // Dropping the connection is what a crash looks like to the compositor.
    drop(greeter);
    state.disconnect(session_id);

    let lock = state
        .session_lock()
        .lock()
        .expect("the lock outlives its client");
    assert!(
        lock.is_abandoned(),
        "the lock is orphaned, waiting to be adopted"
    );
}

#[test]
fn a_process_that_is_not_the_greeter_cannot_claim_its_identity() {
    // The real coordinator, which checks the peer's identity against its
    // process rather than its word. A test binary is not the installed greeter,
    // so claiming to be it is refused before a session even exists.
    let mut state = real_compositor();

    let error = match Greeter::connect(&mut state) {
        Err(error) => error,
        Ok(_) => panic!("an impostor must not be able to connect as the greeter"),
    };
    assert!(
        matches!(error, LockError::Rejected(_)),
        "expected the claim to be rejected, got {error:?}"
    );
    assert!(
        state.session_lock().lock().is_none(),
        "a refused claim must not leave a lock engaged"
    );
}

#[test]
fn an_honest_client_is_still_denied_the_session_lock() {
    // Connecting under this process's real identity succeeds — it is an
    // ordinary client. Asking for the session lock is where it is turned away,
    // because that capability belongs to the greeter alone.
    let mut state = real_compositor();

    let error = match Greeter::connect_as(&mut state, &own_identity()) {
        Err(error) => error,
        Ok(_) => panic!("an ordinary client must not be able to lock the screen"),
    };
    assert!(
        matches!(error, LockError::CapabilityDenied(_)),
        "expected a capability denial, got {error:?}"
    );
    assert!(
        state.session_lock().lock().is_none(),
        "a denied request must not leave a lock engaged"
    );
}

/// A compositor running the real security coordinator.
fn real_compositor() -> ScpState {
    let mut state = ScpState::new();
    state.output_manager_mut().add_output(
        "TEST-1".to_string(),
        "test output".to_string(),
        OUTPUT_WIDTH,
        OUTPUT_HEIGHT,
        60_000,
    );
    state
}

/// The identity the compositor will derive for this test process.
fn own_identity() -> String {
    std::fs::read_to_string(format!("/proc/{}/comm", std::process::id()))
        .expect("read process identity")
        .trim()
        .to_string()
}
