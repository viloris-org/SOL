//! Input routing, popup lifetime, and clipboard mediation in `ScpState`.
//!
//! These drive the compositor state directly rather than over a socket, because
//! the behavior under test is what the state machine decides — which window a
//! click lands on, which popups a grab dismisses, whether a clipboard read is
//! allowed — not how the bytes are framed. Transport framing is covered by
//! `scp_session.rs`.
//!
//! Each client registers a [`SessionSink`] exactly as the transport does, so
//! compositor-initiated events can be observed the same way a real client would
//! receive them.

// `expect` in a test is a deliberate assertion, not an unhandled error.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sol_compositor::scp::{
    ScpState,
    event_queue::{OutboundEvent, SessionSink},
    protocol::{
        ButtonState, ClientMessage, CompositorMessage, ConstraintAdjustment, DismissReason, Edge,
        Gravity, InputEvent, KeyState, PopupPositioner, Rect, SessionId, SurfaceId,
    },
    stack::StackKind,
};
use std::{collections::HashMap, sync::Arc};

/// Escape in the XKB keycode space the compositor speaks (evdev + 8).
const KEY_ESCAPE: u32 = 9;
/// Left mouse button, evdev `BTN_LEFT`.
const BTN_LEFT: u32 = 0x110;

const OUTPUT_WIDTH: i32 = 1920;
const OUTPUT_HEIGHT: i32 = 1080;

/// A connected client, with the outbound queue the transport would own.
struct Client {
    session_id: SessionId,
    sink: Arc<SessionSink>,
    tokens: HashMap<String, Vec<u8>>,
}

impl Client {
    fn token(&self, capability: &str) -> Vec<u8> {
        self.tokens
            .get(capability)
            .unwrap_or_else(|| panic!("capability '{capability}' was not granted"))
            .clone()
    }

    /// Take every event queued for this client since the last drain.
    fn drain(&self) -> Vec<OutboundEvent> {
        self.sink.drain()
    }

    /// Take every queued event, discarding attached descriptors.
    fn drain_messages(&self) -> Vec<CompositorMessage> {
        self.drain()
            .into_iter()
            .map(|event| event.message)
            .collect()
    }
}

/// A compositor with one 1920×1080 output registered.
fn compositor() -> ScpState {
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

fn own_app_id() -> String {
    std::fs::read_to_string(format!("/proc/{}/comm", std::process::id()))
        .expect("read process identity")
        .trim()
        .to_string()
}

fn connect(state: &mut ScpState) -> Client {
    let responses = state
        .handle_message(
            None,
            ClientMessage::Connect {
                app_id: own_app_id(),
                pid: std::process::id(),
            },
        )
        .expect("connect succeeds");

    let (session_id, tokens) = match &responses[0] {
        CompositorMessage::Connected {
            session_id,
            capability_tokens,
            ..
        } => (*session_id, capability_tokens.clone()),
        response => panic!("unexpected connect response: {response:?}"),
    };

    let sink = SessionSink::new().expect("create session sink");
    state.register_session_sink(session_id, Arc::clone(&sink));

    Client {
        session_id,
        sink,
        tokens,
    }
}

/// Request a non-default capability and return its token.
fn request_capability(state: &mut ScpState, client: &Client, capability: &str) -> Vec<u8> {
    let responses = state
        .handle_message(
            Some(client.session_id),
            ClientMessage::RequestCapability {
                capability: capability.to_string(),
                justification: "integration test".to_string(),
            },
        )
        .expect("capability request succeeds");

    match &responses[0] {
        CompositorMessage::CapabilityDecision {
            granted: true,
            token: Some(token),
            ..
        } => token.clone(),
        response => panic!("capability '{capability}' was refused: {response:?}"),
    }
}

fn send(state: &mut ScpState, client: &Client, message: ClientMessage) -> Vec<CompositorMessage> {
    state
        .handle_message(Some(client.session_id), message)
        .expect("request succeeds")
}

/// Create a surface and give it a toplevel role, returning the toplevel id.
fn create_toplevel(state: &mut ScpState, client: &Client, surface_id: SurfaceId) -> u32 {
    send(state, client, ClientMessage::CreateSurface { surface_id });
    let responses = send(
        state,
        client,
        ClientMessage::CreateToplevel {
            surface_id,
            capability_token: client.token("window-toplevel"),
            title: format!("window {surface_id}"),
        },
    );
    match responses[0] {
        CompositorMessage::ConfigureToplevel { toplevel_id, .. } => toplevel_id,
        ref response => panic!("unexpected toplevel response: {response:?}"),
    }
}

fn positioner(size: (i32, i32)) -> PopupPositioner {
    PopupPositioner {
        anchor_rect: Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 30,
        },
        anchor_edge: Edge::Bottom,
        gravity: Gravity::Bottom,
        constraint: ConstraintAdjustment {
            flip_x: false,
            flip_y: false,
            slide_x: false,
            slide_y: false,
            resize_x: false,
            resize_y: false,
        },
        offset: (0, 0),
        size,
    }
}

/// Create a popup on `parent_id`, returning its id.
fn create_popup(
    state: &mut ScpState,
    client: &Client,
    surface_id: SurfaceId,
    parent_id: SurfaceId,
    grab: bool,
) -> u32 {
    send(state, client, ClientMessage::CreateSurface { surface_id });
    let responses = send(
        state,
        client,
        ClientMessage::CreatePopup {
            surface_id,
            parent_id,
            positioner: positioner((120, 200)),
            grab,
        },
    );
    match responses[0] {
        CompositorMessage::ConfigurePopup { popup_id, .. } => popup_id,
        ref response => panic!("unexpected popup response: {response:?}"),
    }
}

fn input_events(messages: &[CompositorMessage]) -> Vec<&InputEvent> {
    messages
        .iter()
        .filter_map(|message| match message {
            CompositorMessage::InputEvent { event, .. } => Some(event),
            _ => None,
        })
        .collect()
}

fn dismissals(messages: &[CompositorMessage]) -> Vec<(u32, DismissReason)> {
    messages
        .iter()
        .filter_map(|message| match message {
            CompositorMessage::PopupDismissed { popup_id, reason } => Some((*popup_id, *reason)),
            _ => None,
        })
        .collect()
}

// ===== Window placement and hit-testing =====

#[test]
fn a_new_toplevel_is_centered_on_the_output() {
    let mut state = compositor();
    let client = connect(&mut state);
    create_toplevel(&mut state, &client, 1);

    let stack = state.build_stack();
    let entry = stack.iter().next().expect("the toplevel is stacked");
    // (1920 - 800) / 2, (1080 - 600) / 2
    assert_eq!((entry.rect.x, entry.rect.y), (560, 240));
    assert_eq!((entry.rect.width, entry.rect.height), (800, 600));
}

#[test]
fn pointer_motion_reports_surface_local_coordinates() {
    let mut state = compositor();
    let client = connect(&mut state);
    create_toplevel(&mut state, &client, 1);
    let _ = client.drain();

    state.handle_pointer_motion(600.0, 300.0, 10);

    let messages = client.drain_messages();
    let events = input_events(&messages);
    match events.first().expect("an enter event was sent") {
        InputEvent::PointerEnter { x, y, .. } => {
            // The window sits at (560, 240), so (600, 300) is (40, 60) inside it.
            assert!((x - 40.0).abs() < f64::EPSILON, "local x was {x}");
            assert!((y - 60.0).abs() < f64::EPSILON, "local y was {y}");
        }
        event => panic!("expected PointerEnter, got {event:?}"),
    }
}

#[test]
fn crossing_between_windows_sends_leave_then_enter() {
    let mut state = compositor();
    let client = connect(&mut state);
    create_toplevel(&mut state, &client, 1);
    create_toplevel(&mut state, &client, 2);

    // The second window cascades by 32px, so a point near the first window's
    // top-left is covered only by the first.
    state.handle_pointer_motion(570.0, 250.0, 10);
    let first = client.drain_messages();
    assert!(
        input_events(&first)
            .iter()
            .any(|event| matches!(event, InputEvent::PointerEnter { .. })),
        "entering the first window: {first:?}"
    );

    // Move deep into the overlap, where the newer window is on top.
    state.handle_pointer_motion(900.0, 500.0, 20);
    let second = client.drain_messages();
    let events = input_events(&second);
    assert!(
        matches!(events.first(), Some(InputEvent::PointerLeave { .. })),
        "leave must precede enter: {second:?}"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, InputEvent::PointerEnter { .. })),
        "entering the second window: {second:?}"
    );
}

#[test]
fn an_empty_input_region_makes_a_window_click_through() {
    let mut state = compositor();
    let client = connect(&mut state);
    create_toplevel(&mut state, &client, 1);
    let covered = create_toplevel(&mut state, &client, 2);

    let point = (900.0, 500.0);
    assert_eq!(
        state
            .hit_test(point.0, point.1)
            .expect("the newer window covers the point")
            .kind,
        StackKind::Toplevel(covered),
    );

    // An explicitly empty input region is how a client opts out of input.
    send(
        &mut state,
        &client,
        ClientMessage::SetInputRegion {
            surface_id: 2,
            rects: Vec::new(),
        },
    );

    let hit = state
        .hit_test(point.0, point.1)
        .expect("input falls through to the window beneath");
    assert_ne!(hit.kind, StackKind::Toplevel(covered));
}

#[test]
fn clicking_a_window_raises_it_and_takes_focus() {
    let mut state = compositor();
    let client = connect(&mut state);
    let first = create_toplevel(&mut state, &client, 1);
    let second = create_toplevel(&mut state, &client, 2);

    // The newest window starts on top.
    assert_eq!(
        state.build_stack().iter().next().map(|entry| entry.kind),
        Some(StackKind::Toplevel(second))
    );

    // Click a point covered only by the older window.
    state.handle_pointer_motion(570.0, 250.0, 10);
    state.handle_pointer_button(BTN_LEFT, ButtonState::Pressed, 20);

    assert_eq!(
        state.build_stack().iter().next().map(|entry| entry.kind),
        Some(StackKind::Toplevel(first)),
        "clicking a window raises it"
    );
    assert_eq!(state.get_focused_surface(), Some((client.session_id, 1)));
}

#[test]
fn clicking_empty_desktop_clears_focus() {
    let mut state = compositor();
    let client = connect(&mut state);
    create_toplevel(&mut state, &client, 1);
    assert!(state.get_focused_surface().is_some());

    // The far corner is outside the centered 800×600 window.
    state.handle_pointer_motion(10.0, 10.0, 10);
    state.handle_pointer_button(BTN_LEFT, ButtonState::Pressed, 20);

    assert_eq!(
        state.get_focused_surface(),
        None,
        "focus must not survive a click on nothing"
    );
}

// ===== Popup lifetime =====

#[test]
fn a_popup_is_positioned_relative_to_its_parent() {
    let mut state = compositor();
    let client = connect(&mut state);
    create_toplevel(&mut state, &client, 1);
    let popup = create_popup(&mut state, &client, 2, 1, true);

    let stack = state.build_stack();
    let entry = stack
        .iter()
        .find(|entry| entry.kind == StackKind::Popup(popup))
        .expect("the popup is stacked");

    // Parent at (560, 240); anchor bottom-center of a 100×30 rect at (0, 0)
    // is (50, 30) parent-local, and gravity Bottom centers a 120-wide popup
    // on it: (50 - 60, 30) → absolute (550, 270).
    assert_eq!((entry.rect.x, entry.rect.y), (550, 270));
    assert_eq!((entry.rect.width, entry.rect.height), (120, 200));
}

#[test]
fn a_popup_stacks_above_its_parent() {
    let mut state = compositor();
    let client = connect(&mut state);
    create_toplevel(&mut state, &client, 1);
    let popup = create_popup(&mut state, &client, 2, 1, true);

    // (600, 300) is inside both the popup and the parent window.
    assert_eq!(
        state.hit_test(600.0, 300.0).expect("point is covered").kind,
        StackKind::Popup(popup),
    );
}

/// Create a popup with a caller-chosen positioner, returning the configure.
fn create_popup_with(
    state: &mut ScpState,
    client: &Client,
    surface_id: SurfaceId,
    parent_id: SurfaceId,
    positioner: PopupPositioner,
) -> Vec<CompositorMessage> {
    send(state, client, ClientMessage::CreateSurface { surface_id });
    send(
        state,
        client,
        ClientMessage::CreatePopup {
            surface_id,
            parent_id,
            positioner,
            grab: true,
        },
    )
}

#[test]
fn an_oversized_popup_is_confined_to_the_output() {
    let mut state = compositor();
    let client = connect(&mut state);
    create_toplevel(&mut state, &client, 1);

    // A popup far larger than the screen, pushed far off its top-left corner.
    let mut hostile = positioner((10_000, 10_000));
    hostile.offset = (-4_000, -4_000);
    create_popup_with(&mut state, &client, 2, 1, hostile);

    let entry = state
        .build_stack()
        .iter()
        .find(|entry| matches!(entry.kind, StackKind::Popup(_)))
        .copied()
        .expect("popup is stacked");

    assert!(
        entry.rect.width <= OUTPUT_WIDTH && entry.rect.height <= OUTPUT_HEIGHT,
        "a popup must not exceed the output it appears on: {:?}",
        entry.rect
    );
}

#[test]
fn a_popup_cannot_cover_a_window_stacked_above_its_parent() {
    let mut state = compositor();
    let attacker = connect(&mut state);
    let victim = connect(&mut state);

    create_toplevel(&mut state, &attacker, 1);
    let mut wide = positioner((OUTPUT_WIDTH, OUTPUT_HEIGHT));
    wide.offset = (-OUTPUT_WIDTH, -OUTPUT_HEIGHT);
    create_popup_with(&mut state, &attacker, 2, 1, wide);

    // The victim's window is raised, so it sits above the attacker's window —
    // and therefore above anything hanging off it.
    create_toplevel(&mut state, &victim, 1);

    // A point inside the victim's window, which the attacker's popup spans.
    let hit = state
        .hit_test(700.0, 400.0)
        .expect("the victim's window covers this point");
    assert_eq!(
        hit.session_id, victim.session_id,
        "a popup must not take input over a window stacked above its parent: {hit:?}"
    );
}

#[test]
fn a_hostile_positioner_is_refused_rather_than_wrapping() {
    let mut state = compositor();
    let client = connect(&mut state);
    create_toplevel(&mut state, &client, 1);

    // Every field at its extreme: plain arithmetic on these overflowed, which
    // panicked inside the compositor state lock under overflow checks and
    // produced an arbitrary on-screen rectangle without them.
    let mut hostile = positioner((i32::MAX, i32::MAX));
    hostile.anchor_rect = Rect {
        x: i32::MAX,
        y: i32::MAX,
        width: i32::MAX,
        height: i32::MAX,
    };
    hostile.offset = (i32::MAX, i32::MAX);
    create_popup_with(&mut state, &client, 2, 1, hostile);

    let entry = state
        .build_stack()
        .iter()
        .find(|entry| matches!(entry.kind, StackKind::Popup(_)))
        .copied()
        .expect("popup is stacked");
    assert!(
        entry.rect.width <= OUTPUT_WIDTH && entry.rect.height <= OUTPUT_HEIGHT,
        "saturated geometry must still be confined: {:?}",
        entry.rect
    );
}

#[test]
fn nested_popup_offsets_accumulate_through_the_parent_chain() {
    let mut state = compositor();
    let client = connect(&mut state);
    create_toplevel(&mut state, &client, 1);
    let outer = create_popup(&mut state, &client, 2, 1, true);
    let inner = create_popup(&mut state, &client, 3, 2, true);

    let stack = state.build_stack();
    let outer_rect = stack
        .iter()
        .find(|entry| entry.kind == StackKind::Popup(outer))
        .expect("outer popup is stacked")
        .rect;
    let inner_rect = stack
        .iter()
        .find(|entry| entry.kind == StackKind::Popup(inner))
        .expect("inner popup is stacked")
        .rect;

    // Both popups use the same positioner, so the submenu sits one offset
    // further along than the menu that opened it.
    assert_eq!(inner_rect.x - outer_rect.x, outer_rect.x - 560);
    assert_eq!(inner_rect.y - outer_rect.y, outer_rect.y - 240);
}

#[test]
fn a_submenu_stacks_above_the_menu_that_opened_it() {
    let mut state = compositor();
    let client = connect(&mut state);
    create_toplevel(&mut state, &client, 1);
    create_popup(&mut state, &client, 2, 1, true);
    let inner = create_popup(&mut state, &client, 3, 2, true);

    let stack = state.build_stack();
    let inner_rect = stack
        .iter()
        .find(|entry| entry.kind == StackKind::Popup(inner))
        .expect("inner popup is stacked")
        .rect;

    // A point inside the innermost popup must resolve to it, not to the popup
    // underneath.
    let probe = (f64::from(inner_rect.x) + 5.0, f64::from(inner_rect.y) + 5.0);
    assert_eq!(
        state
            .hit_test(probe.0, probe.1)
            .expect("point is covered")
            .kind,
        StackKind::Popup(inner),
    );
}

#[test]
fn clicking_outside_a_grab_dismisses_the_whole_chain() {
    let mut state = compositor();
    let client = connect(&mut state);
    create_toplevel(&mut state, &client, 1);
    let outer = create_popup(&mut state, &client, 2, 1, true);
    let inner = create_popup(&mut state, &client, 3, 2, true);
    let _ = client.drain();

    // A point inside the parent window but outside every popup.
    state.handle_pointer_motion(1300.0, 800.0, 10);
    let _ = client.drain();
    state.handle_pointer_button(BTN_LEFT, ButtonState::Pressed, 20);

    let messages = client.drain_messages();
    let dismissed = dismissals(&messages);
    assert_eq!(
        dismissed,
        vec![
            (inner, DismissReason::OutsideClick),
            (outer, DismissReason::OutsideClick),
        ],
        "the chain collapses innermost-first: {messages:?}"
    );
    assert!(state.popups().is_empty());

    // The grab consumed the click, so no button reached the window behind it.
    assert!(
        !input_events(&messages)
            .iter()
            .any(|event| matches!(event, InputEvent::PointerButton { .. })),
        "a grabbed click must not fall through: {messages:?}"
    );
}

#[test]
fn clicking_an_outer_menu_collapses_only_its_submenus() {
    let mut state = compositor();
    let client = connect(&mut state);
    create_toplevel(&mut state, &client, 1);
    let outer = create_popup(&mut state, &client, 2, 1, true);
    let inner = create_popup(&mut state, &client, 3, 2, true);

    let outer_rect = state
        .build_stack()
        .iter()
        .find(|entry| entry.kind == StackKind::Popup(outer))
        .expect("outer popup is stacked")
        .rect;
    let inner_rect = state
        .build_stack()
        .iter()
        .find(|entry| entry.kind == StackKind::Popup(inner))
        .expect("inner popup is stacked")
        .rect;

    // Aim at the outer popup's top-left, which the submenu's offset leaves clear.
    let probe = (f64::from(outer_rect.x) + 2.0, f64::from(outer_rect.y) + 2.0);
    assert!(
        probe.0 < f64::from(inner_rect.x) || probe.1 < f64::from(inner_rect.y),
        "probe must not be covered by the submenu"
    );

    state.handle_pointer_motion(probe.0, probe.1, 10);
    let _ = client.drain();
    state.handle_pointer_button(BTN_LEFT, ButtonState::Pressed, 20);

    let messages = client.drain_messages();
    assert_eq!(
        dismissals(&messages),
        vec![(inner, DismissReason::OutsideClick)],
        "only the submenu closes: {messages:?}"
    );
    assert!(
        state.popups().get(outer).is_some(),
        "the menu under the cursor stays open"
    );
    // Unlike a click outside the chain, this one is still delivered.
    assert!(
        input_events(&messages)
            .iter()
            .any(|event| matches!(event, InputEvent::PointerButton { .. })),
        "a click inside the chain is delivered: {messages:?}"
    );
}

#[test]
fn escape_dismisses_the_innermost_popup_only() {
    let mut state = compositor();
    let client = connect(&mut state);
    create_toplevel(&mut state, &client, 1);
    let outer = create_popup(&mut state, &client, 2, 1, true);
    let inner = create_popup(&mut state, &client, 3, 2, true);
    let _ = client.drain();

    state.handle_key(KEY_ESCAPE, KeyState::Pressed, 10);

    let messages = client.drain_messages();
    assert_eq!(
        dismissals(&messages),
        vec![(inner, DismissReason::EscapeKey)],
        "escape closes one level: {messages:?}"
    );
    assert!(state.popups().get(outer).is_some());

    // A second Escape closes the menu that is now innermost.
    state.handle_key(KEY_ESCAPE, KeyState::Pressed, 20);
    assert_eq!(
        dismissals(&client.drain_messages()),
        vec![(outer, DismissReason::EscapeKey)]
    );
    assert!(state.popups().is_empty());
}

#[test]
fn escape_reaches_the_client_when_no_popup_is_grabbing() {
    let mut state = compositor();
    let client = connect(&mut state);
    create_toplevel(&mut state, &client, 1);
    let _ = client.drain();

    state.handle_key(KEY_ESCAPE, KeyState::Pressed, 10);

    let messages = client.drain_messages();
    assert!(
        input_events(&messages).iter().any(|event| matches!(
            event,
            InputEvent::KeyboardKey {
                key: KEY_ESCAPE,
                ..
            }
        )),
        "escape is only intercepted for popup grabs: {messages:?}"
    );
}

#[test]
fn destroying_a_parent_surface_cascades_popup_dismissal() {
    let mut state = compositor();
    let client = connect(&mut state);
    create_toplevel(&mut state, &client, 1);
    let outer = create_popup(&mut state, &client, 2, 1, true);
    let inner = create_popup(&mut state, &client, 3, 2, true);
    let _ = client.drain();

    send(
        &mut state,
        &client,
        ClientMessage::DestroySurface { surface_id: 1 },
    );

    let messages = client.drain_messages();
    let dismissed = dismissals(&messages);
    assert!(
        dismissed.contains(&(inner, DismissReason::ParentClosed))
            && dismissed.contains(&(outer, DismissReason::ParentClosed)),
        "both popups must be dismissed: {messages:?}"
    );
    assert!(
        state.popups().is_empty(),
        "no popup may outlive the surface it hangs off"
    );
}

#[test]
fn a_popup_cannot_be_parented_to_a_roleless_surface() {
    let mut state = compositor();
    let client = connect(&mut state);
    send(
        &mut state,
        &client,
        ClientMessage::CreateSurface { surface_id: 1 },
    );
    send(
        &mut state,
        &client,
        ClientMessage::CreateSurface { surface_id: 2 },
    );

    let error = state
        .handle_message(
            Some(client.session_id),
            ClientMessage::CreatePopup {
                surface_id: 2,
                parent_id: 1,
                positioner: positioner((120, 200)),
                grab: true,
            },
        )
        .expect_err("a parent with no role has no resolvable position");
    assert!(
        error.contains("must be a toplevel"),
        "unexpected error: {error}"
    );
}

#[test]
fn popups_are_isolated_between_clients_using_the_same_surface_ids() {
    let mut state = compositor();
    let first = connect(&mut state);
    let second = connect(&mut state);

    // Both clients use surface IDs 1 and 2: IDs are client-local.
    create_toplevel(&mut state, &first, 1);
    let first_popup = create_popup(&mut state, &first, 2, 1, true);
    create_toplevel(&mut state, &second, 1);
    let second_popup = create_popup(&mut state, &second, 2, 1, true);

    assert_ne!(first_popup, second_popup);

    let error = state
        .handle_message(
            Some(second.session_id),
            ClientMessage::DestroyPopup {
                popup_id: first_popup,
            },
        )
        .expect_err("a client must not destroy another client's popup");
    assert!(
        error.contains("does not belong"),
        "unexpected error: {error}"
    );

    // Destroying one client's popup leaves the other's intact.
    send(
        &mut state,
        &first,
        ClientMessage::DestroyPopup {
            popup_id: first_popup,
        },
    );
    assert!(state.popups().get(first_popup).is_none());
    assert!(state.popups().get(second_popup).is_some());
}

// ===== Toplevel lifetime =====

#[test]
fn closing_a_toplevel_notifies_the_client_and_unstacks_it() {
    let mut state = compositor();
    let client = connect(&mut state);
    let toplevel = create_toplevel(&mut state, &client, 1);
    let _ = client.drain();

    send(
        &mut state,
        &client,
        ClientMessage::CloseToplevel {
            toplevel_id: toplevel,
        },
    );

    let messages = client.drain_messages();
    assert!(
        messages.iter().any(|message| matches!(
            message,
            CompositorMessage::ToplevelClosed { toplevel_id } if *toplevel_id == toplevel
        )),
        "the client must be told its window closed: {messages:?}"
    );
    assert!(
        state
            .build_stack()
            .iter()
            .all(|entry| entry.kind != StackKind::Toplevel(toplevel))
    );
}

#[test]
fn closing_the_focused_window_hands_focus_to_the_next_one() {
    let mut state = compositor();
    let client = connect(&mut state);
    let first = create_toplevel(&mut state, &client, 1);
    let second = create_toplevel(&mut state, &client, 2);

    // The newest window has focus; closing it should fall back to the older one.
    assert_eq!(state.get_focused_surface(), Some((client.session_id, 2)));
    send(
        &mut state,
        &client,
        ClientMessage::CloseToplevel {
            toplevel_id: second,
        },
    );

    assert_eq!(
        state.get_focused_surface(),
        Some((client.session_id, 1)),
        "focus falls back to the remaining window"
    );
    assert_eq!(
        state.build_stack().iter().next().map(|entry| entry.kind),
        Some(StackKind::Toplevel(first))
    );
}

#[test]
fn a_minimized_window_stops_receiving_input() {
    use sol_compositor::scp::protocol::ToplevelStateRequest;

    let mut state = compositor();
    let client = connect(&mut state);
    let toplevel = create_toplevel(&mut state, &client, 1);

    assert!(state.hit_test(600.0, 300.0).is_some());
    send(
        &mut state,
        &client,
        ClientMessage::SetToplevelState {
            toplevel_id: toplevel,
            state: ToplevelStateRequest::Minimize,
        },
    );

    assert!(
        state.hit_test(600.0, 300.0).is_none(),
        "an off-screen window must not swallow input"
    );
    assert_eq!(state.get_focused_surface(), None);
}

// ===== Clipboard mediation =====

/// Click `client`'s window and return the serial the compositor issued.
///
/// A client cannot invent this: privileged requests must quote the serial of an
/// input event the compositor actually sent them.
fn click_for_serial(state: &mut ScpState, client: &Client) -> u32 {
    state.handle_pointer_motion(600.0, 300.0, 10);
    let _ = client.drain();
    state.handle_pointer_button(BTN_LEFT, ButtonState::Pressed, 20);

    let messages = client.drain_messages();
    input_events(&messages)
        .iter()
        .find_map(|event| match event {
            InputEvent::PointerButton { serial, .. } => Some(*serial),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no button event reached the client: {messages:?}"))
}

/// Offer a selection, first generating the interaction the write requires.
fn offer_selection(state: &mut ScpState, client: &Client, mime: &str) {
    let serial = click_for_serial(state, client);
    state
        .handle_message(
            Some(client.session_id),
            ClientMessage::SetSelection {
                mime_types: vec![mime.to_string()],
                serial,
            },
        )
        .expect("selection is accepted after real input");
}

#[test]
fn a_clipboard_write_without_recent_interaction_is_refused() {
    let mut state = compositor();
    let client = connect(&mut state);
    create_toplevel(&mut state, &client, 1);

    let serial = state.next_serial();
    let error = state
        .handle_message(
            Some(client.session_id),
            ClientMessage::SetSelection {
                mime_types: vec!["text/plain".to_string()],
                serial,
            },
        )
        .expect_err("a clipboard write needs recent user input");
    assert!(
        error.contains("user interaction"),
        "unexpected error: {error}"
    );
}

#[test]
fn a_clipboard_write_after_a_click_is_offered_to_other_clients() {
    let mut state = compositor();
    let owner = connect(&mut state);
    let peer = connect(&mut state);
    create_toplevel(&mut state, &owner, 1);
    let _ = peer.drain();

    offer_selection(&mut state, &owner, "text/plain");

    let peer_messages = peer.drain_messages();
    assert!(
        peer_messages.iter().any(|message| matches!(
            message,
            CompositorMessage::SelectionOffer { mime_types } if mime_types == &["text/plain"]
        )),
        "other clients learn an offer exists: {peer_messages:?}"
    );
    // The offer announces availability only — no content crosses yet.
    assert!(
        owner
            .drain()
            .iter()
            .all(|event| !matches!(event.message, CompositorMessage::SelectionOffer { .. })),
        "the owner is not offered its own selection"
    );
}

#[test]
fn a_clipboard_read_from_a_background_client_is_refused() {
    let mut state = compositor();
    let owner = connect(&mut state);
    let peer = connect(&mut state);
    create_toplevel(&mut state, &owner, 1);
    offer_selection(&mut state, &owner, "text/plain");

    // The peer holds the capability but has never had focus.
    request_capability(&mut state, &peer, "clipboard-read");
    let error = state
        .handle_message(
            Some(peer.session_id),
            ClientMessage::RequestSelection {
                mime_type: "text/plain".to_string(),
            },
        )
        .expect_err("a background client must not read the clipboard");
    assert!(
        error.contains("foreground focus"),
        "unexpected error: {error}"
    );
}

#[test]
fn a_focused_clipboard_read_hands_both_ends_of_a_pipe_across() {
    use std::io::{Read, Write};
    use std::os::fd::{FromRawFd, IntoRawFd, OwnedFd};

    let mut state = compositor();
    let owner = connect(&mut state);
    let reader = connect(&mut state);
    create_toplevel(&mut state, &owner, 1);
    offer_selection(&mut state, &owner, "text/plain");

    // The reader needs the capability and focus, which a window plus a click give.
    request_capability(&mut state, &reader, "clipboard-read");
    create_toplevel(&mut state, &reader, 1);
    let _ = owner.drain();
    let _ = reader.drain();

    send(
        &mut state,
        &reader,
        ClientMessage::RequestSelection {
            mime_type: "text/plain".to_string(),
        },
    );

    // The owner is asked to produce the data and given the write end.
    let write_fd = take_fd(&owner, "RequestSelectionData", |message| {
        matches!(message, CompositorMessage::RequestSelectionData { .. })
    });
    // The reader receives the matching read end.
    let read_fd = take_fd(&reader, "SelectionData", |message| {
        matches!(message, CompositorMessage::SelectionData { .. })
    });

    // The compositor kept neither end: content flows client to client.
    let mut producer = unsafe { std::fs::File::from_raw_fd(write_fd.into_raw_fd()) };
    producer
        .write_all(b"clipboard payload")
        .expect("owner writes");
    drop(producer);

    let mut consumer = unsafe { std::fs::File::from_raw_fd(read_fd.into_raw_fd()) };
    let mut received = String::new();
    consumer
        .read_to_string(&mut received)
        .expect("reader reads");
    assert_eq!(received, "clipboard payload");

    /// Pull the descriptor off the one queued event matching `predicate`.
    fn take_fd(
        client: &Client,
        label: &str,
        predicate: impl Fn(&CompositorMessage) -> bool,
    ) -> OwnedFd {
        client
            .drain()
            .into_iter()
            .find(|event| predicate(&event.message))
            .unwrap_or_else(|| panic!("{label} was never queued"))
            .fd
            .unwrap_or_else(|| panic!("{label} carried no descriptor"))
    }
}

#[test]
fn a_clipboard_read_for_an_unoffered_type_is_refused() {
    let mut state = compositor();
    let owner = connect(&mut state);
    let reader = connect(&mut state);
    create_toplevel(&mut state, &owner, 1);
    offer_selection(&mut state, &owner, "text/plain");

    request_capability(&mut state, &reader, "clipboard-read");
    create_toplevel(&mut state, &reader, 1);

    let error = state
        .handle_message(
            Some(reader.session_id),
            ClientMessage::RequestSelection {
                mime_type: "image/png".to_string(),
            },
        )
        .expect_err("only offered MIME types may be read");
    assert!(error.contains("image/png"), "unexpected error: {error}");
}

#[test]
fn a_disconnecting_owner_clears_its_selection() {
    let mut state = compositor();
    let owner = connect(&mut state);
    let peer = connect(&mut state);
    create_toplevel(&mut state, &owner, 1);
    offer_selection(&mut state, &owner, "text/plain");
    let _ = peer.drain();

    state.disconnect(owner.session_id);

    let messages = peer.drain_messages();
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, CompositorMessage::SelectionCleared)),
        "clipboard content cannot outlive the client that owns it: {messages:?}"
    );

    // And a read now finds nothing rather than a dangling owner.
    request_capability(&mut state, &peer, "clipboard-read");
    create_toplevel(&mut state, &peer, 1);
    let error = state
        .handle_message(
            Some(peer.session_id),
            ClientMessage::RequestSelection {
                mime_type: "text/plain".to_string(),
            },
        )
        .expect_err("the selection is gone");
    assert!(error.contains("No selection"), "unexpected error: {error}");
}

// ===== Frame callbacks =====

#[test]
fn frame_callbacks_are_delivered_after_a_commit() {
    let mut state = compositor();
    let client = connect(&mut state);
    create_toplevel(&mut state, &client, 1);
    send(
        &mut state,
        &client,
        ClientMessage::Commit {
            surface_id: 1,
            frame_callback: Some(77),
        },
    );
    let _ = client.drain();

    assert_eq!(state.send_frame_callbacks(1_234), 1);

    let messages = client.drain_messages();
    assert!(
        messages.iter().any(|message| matches!(
            message,
            CompositorMessage::FrameCallback {
                callback_id: 77,
                timestamp_ms: 1_234,
                ..
            }
        )),
        "the callback must reach the client: {messages:?}"
    );

    // Callbacks fire once; the next frame has nothing pending.
    assert_eq!(state.send_frame_callbacks(2_345), 0);
}
