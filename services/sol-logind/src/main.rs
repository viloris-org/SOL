//! The login screen, presented on an SCP session-lock surface.
//!
//! The compositor must already be running: this process connects to it, engages
//! the session lock, and draws the login UI into a shared buffer. It never opens
//! a display of its own.

use std::{
    cell::RefCell,
    env,
    rc::Rc,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use sol_logind::{
    FrameBuffer, KeyInput, LoginAction, LoginRenderer, LoginService, Modifiers, ScpClient,
    SessionHandoff,
    scp::{
        keys,
        lock::{BTN_LEFT, LockEvent},
    },
};
use tracing::{info, warn};

/// Longest wait between compositor polls while nothing is animating.
///
/// The login screen is idle almost all of the time — it redraws on a keystroke,
/// not on a clock — so a long sleep here costs nothing and keeps a greeter from
/// spinning on an otherwise empty machine.
const IDLE_POLL: Duration = Duration::from_millis(250);
const HANDOFF_FRAME_INTERVAL: Duration = Duration::from_millis(16);
const DESKTOP_READY_TIMEOUT: Duration = Duration::from_secs(15);

/// What the user is told when authentication fails.
///
/// Deliberately the same whether the account exists, the password is wrong, or
/// PAM refused for its own reasons: a login screen that distinguishes them is a
/// user-enumeration oracle.
const AUTH_FAILED: &str = "Incorrect user name or password.";
const SESSION_FAILED: &str = "Your desktop could not be started. Please try again.";

fn main() -> Result<()> {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    } else {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .init();
    }

    info!("SOL login service starting");

    let dev_mode = env::args().any(|arg| arg == "--dev" || arg == "--development");
    let service = if dev_mode {
        info!("running in DEVELOPMENT mode (mock users, stub auth)");
        Rc::new(RefCell::new(LoginService::new_development()?))
    } else {
        info!("running in PRODUCTION mode (system users, PAM auth)");
        Rc::new(RefCell::new(LoginService::new()?))
    };

    service.borrow_mut().start()?;
    info!(
        users = service.borrow().ui.users.len(),
        "login screen initialized"
    );

    // Take the screen before drawing anything: the lock engages the moment the
    // compositor accepts it, which is what stops the desktop underneath from
    // still receiving input while the greeter is starting up.
    let mut client = ScpClient::connect().context("could not present the login screen over SCP")?;

    let renderer = LoginRenderer::new()
        .map_err(|error| anyhow::anyhow!("could not create the login renderer: {error}"))?;
    connect_pointer_controls(&renderer, &service);

    let (width, height) = client.size();
    let mut buffer =
        FrameBuffer::new(width, height).context("could not allocate the login frame buffer")?;
    renderer.resize(width, height);

    run(&mut client, &renderer, &service, &mut buffer, dev_mode)?;

    info!("login service exiting");
    Ok(())
}

/// Route the controls Slint owns the hit-testing for back into the login state.
fn connect_pointer_controls(renderer: &LoginRenderer, service: &Rc<RefCell<LoginService>>) {
    renderer.connect(
        {
            let service = Rc::clone(service);
            move |index| {
                info!(index, "user selected");
                service.borrow_mut().ui.select_user(index);
            }
        },
        {
            let service = Rc::clone(service);
            move || service.borrow_mut().ui.toggle_password_visibility()
        },
    );
}

fn run(
    client: &mut ScpClient,
    renderer: &LoginRenderer,
    service: &Rc<RefCell<LoginService>>,
    buffer: &mut FrameBuffer,
    dev_mode: bool,
) -> Result<()> {
    // Until settingsd owns the pre-login appearance, use the same default as
    // the desktop shell so the handoff cannot flash between light and dark.
    let mode = sol_design::accessibility::TokenMode::dark();
    let mut modifiers = Modifiers::default();

    loop {
        let mut requested = None;

        for event in client.poll(IDLE_POLL)? {
            match event {
                LockEvent::Locked => {
                    renderer.set_active(true);
                    renderer.invalidate();
                }
                LockEvent::Resized { width, height } => {
                    buffer
                        .resize(width, height)
                        .context("could not resize the login frame buffer")?;
                    renderer.resize(width, height);
                }
                LockEvent::Modifiers {
                    depressed,
                    latched,
                    locked,
                } => modifiers = Modifiers::from_masks(depressed, latched, locked),
                LockEvent::FocusChanged(active) => {
                    // Held modifiers cannot be tracked across a focus gap, so
                    // they are dropped rather than left to apply to the next
                    // keystroke.
                    if !active {
                        modifiers = Modifiers::default();
                    }
                    renderer.set_active(active);
                }
                LockEvent::Key {
                    keycode,
                    pressed: true,
                } => {
                    if let Some(action) = apply_key(keycode, modifiers, service) {
                        requested = Some(action);
                    }
                }
                LockEvent::Key { pressed: false, .. } => {}
                LockEvent::PointerMoved { x, y } => renderer.pointer_moved(x, y),
                LockEvent::PointerButton { button, pressed } if button == BTN_LEFT => {
                    renderer.pointer_button(pressed);
                }
                LockEvent::PointerButton { .. } | LockEvent::Frame => {}
                LockEvent::Finished { reason } => {
                    // The compositor took the lock away. Nothing this process
                    // draws is on screen any more, so do not keep pretending to
                    // guard a session it no longer owns.
                    warn!(%reason, "the compositor withdrew the session lock");
                    return Ok(());
                }
            }
        }

        renderer.tick();
        renderer.render(&service.borrow().ui.frame_for(mode));
        if renderer.draw_into(buffer) {
            client.present(buffer)?;
        }

        // A click on the login button lands in the renderer, Enter lands in
        // `apply_key`; either way it is the same request.
        if let Some(LoginAction::Authenticate) = requested.or_else(|| renderer.take_action()) {
            authenticate(client, renderer, service, buffer, mode, dev_mode)?;
        }
    }
}

/// Apply one keystroke to the login state.
///
/// Keys reach `LoginUi` directly rather than going through Slint: the password is
/// this service's to hold, and nothing is gained by routing it through a UI
/// toolkit's text editing.
fn apply_key(
    keycode: u32,
    modifiers: Modifiers,
    service: &Rc<RefCell<LoginService>>,
) -> Option<LoginAction> {
    let input = keys::decode(keycode, modifiers)?;
    let mut service = service.borrow_mut();
    let ui = &mut service.ui;

    match input {
        KeyInput::Char(character) => ui.push_password_char(character),
        KeyInput::Backspace => ui.backspace(),
        KeyInput::Escape => ui.clear_password(),
        KeyInput::NextUser => ui.select_next_user(),
        KeyInput::PreviousUser => ui.select_previous_user(),
        KeyInput::Enter => {
            if ui.can_login() {
                return Some(LoginAction::Authenticate);
            }
        }
    }
    None
}

/// Authenticate, and on success hand the screen to the user's session.
fn authenticate(
    client: &mut ScpClient,
    renderer: &LoginRenderer,
    service: &Rc<RefCell<LoginService>>,
    buffer: &mut FrameBuffer,
    mode: sol_design::accessibility::TokenMode,
    dev_mode: bool,
) -> Result<()> {
    let outcome = service.borrow_mut().authenticate();
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(error) => {
            warn!(%error, "authentication failed");
            let mut service = service.borrow_mut();
            service.ui.reset();
            service.ui.set_status(AUTH_FAILED);
            return Ok(());
        }
    };

    info!(
        user = %outcome.token.username,
        session = %outcome.token.session_id,
        "authentication succeeded"
    );

    let user = service
        .borrow()
        .user_service
        .find_user(&outcome.token.username)
        .cloned();
    let Some(user) = user else {
        // Authentication passed for a name this greeter cannot resolve to an
        // account, so there is nothing to launch. Stay locked rather than
        // releasing the screen to nobody.
        warn!(
            user = %outcome.token.username,
            "no matching account; keeping the session locked"
        );
        let mut service = service.borrow_mut();
        service.ui.reset();
        service.ui.set_status(AUTH_FAILED);
        return Ok(());
    };

    {
        let mut service = service.borrow_mut();
        service.ui.clear_password();
        service.ui.set_status("Preparing your desktop…");
    }

    // The desktop starts behind the still-engaged lock surface. Releasing the
    // lock before the shell has committed a frame would turn process startup
    // time into a visible blank screen.
    let mut pending = match sol_logind::start_user_session(&user, dev_mode) {
        Ok(pending) => pending,
        Err(error) => {
            warn!(%error, "could not start the user session");
            if let Some(session) = outcome.session {
                session.close();
            }
            session_failed(service);
            return Ok(());
        }
    };

    if let Err(error) = pending.wait_until_ready(DESKTOP_READY_TIMEOUT) {
        warn!(%error, "desktop did not become ready; keeping the screen locked");
        pending.abort();
        if let Some(session) = outcome.session {
            session.close();
        }
        session_failed(service);
        return Ok(());
    }

    play_handoff(client, renderer, service, buffer, mode)?;

    // Unlock is the commit point: there is now a ready desktop underneath and
    // the greeter has presented the final transparent-content handoff frame.
    if let Err(error) = client.unlock() {
        pending.abort();
        if let Some(session) = outcome.session {
            session.close();
        }
        return Err(error.into());
    }

    match pending.wait() {
        Ok(status) => info!(%status, "user session exited"),
        Err(error) => warn!(%error, "could not wait for the user session"),
    }

    // The PAM session had to stay open for the whole desktop session above;
    // close it now that it has ended.
    if let Some(session) = outcome.session {
        info!(user = %user.username, "closing PAM session");
        session.close();
    }

    // The desktop is gone, so take the screen back.
    service.borrow_mut().ui.reset();
    service.borrow_mut().ui.clear_password();
    client.relock()?;

    let (width, height) = client.size();
    buffer
        .resize(width, height)
        .context("could not resize the login frame buffer")?;
    renderer.resize(width, height);
    renderer.invalidate();
    Ok(())
}

fn play_handoff(
    client: &mut ScpClient,
    renderer: &LoginRenderer,
    service: &Rc<RefCell<LoginService>>,
    buffer: &mut FrameBuffer,
    mode: sol_design::accessibility::TokenMode,
) -> Result<()> {
    let handoff = SessionHandoff::new(mode);
    let started = Instant::now();
    renderer.invalidate();

    loop {
        let visual = handoff.visual_at(started.elapsed());
        let frame = service.borrow().ui.frame_for_handoff(mode, visual);
        renderer.render(&frame);
        renderer.tick();
        if renderer.draw_into(buffer) {
            client.present(buffer)?;
        }
        if visual.finished {
            return Ok(());
        }

        let remaining = handoff.duration().saturating_sub(started.elapsed());
        thread::sleep(HANDOFF_FRAME_INTERVAL.min(remaining));
    }
}

fn session_failed(service: &Rc<RefCell<LoginService>>) {
    let mut service = service.borrow_mut();
    service.ui.reset();
    service.ui.set_status(SESSION_FAILED);
}
