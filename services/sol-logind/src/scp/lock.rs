//! The session-lock handshake, without a socket in sight.
//!
//! `sol-logind` is the only client the compositor will let engage the session
//! lock (`Capability::SessionLock`, reserved by name in
//! `sol_compositor::scp::security`). This module drives that handshake as a pure
//! state machine: it consumes [`CompositorMessage`]s and produces
//! [`ClientMessage`]s, and never touches a descriptor or a stream.
//!
//! Keeping the transport out means the whole login handshake can be tested
//! against a real [`ScpState`](sol_compositor::scp::ScpState) in-process, the
//! way `compositor/tests/scp_input.rs` tests input routing.
//!
//! ```text
//! Connect                  → Connected{capability_tokens}
//! RequestCapability        → CapabilityDecision{token}      (if not already granted)
//! LockSession{token}       → SessionLockEngaged{lock_id}
//! CreateSurface
//! CreateLockSurface        → ConfigureLockSurface{serial, w, h}
//! AckLockConfigure         → SessionLocked
//!   AttachBuffer/Damage/Commit …   InputEvent …
//! UnlockSession
//! ```

use std::{fmt, os::fd::RawFd};

use sol_compositor::scp::protocol::{
    ButtonState, ClientMessage, CompositorMessage, InputEvent, KeyState, LockId, LockSurfaceId,
    OutputId, Rect, SurfaceId,
};

/// Wire name of the capability that gates the lock.
const SESSION_LOCK: &str = "session-lock";

/// Why the greeter wants the lock, recorded in the compositor's audit trail.
const JUSTIFICATION: &str = "Present the SOL login screen";

/// How far along the handshake is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockPhase {
    /// Nothing sent yet.
    Idle,
    /// `Connect` sent, waiting to be admitted.
    Connecting,
    /// Asking for `session-lock`, because it was not granted at connect.
    RequestingCapability,
    /// `LockSession` sent. The desktop has already stopped receiving input.
    Engaging,
    /// Lock surfaces created, waiting for every output to be covered.
    Presenting,
    /// Locked: this client owns the screen and the keyboard.
    Locked,
    /// `UnlockSession` sent; the desktop is back.
    Released,
    /// The compositor refused or withdrew the lock.
    Finished(String),
}

/// Something the login UI needs to react to.
#[derive(Debug, Clone, PartialEq)]
pub enum LockEvent {
    /// Every output is covered; the login screen is on screen and focused.
    Locked,
    /// The lock surface's size was set or changed.
    Resized {
        width: i32,
        height: i32,
    },
    /// A key went down or came up. Keycodes are XKB (evdev + 8).
    Key {
        keycode: u32,
        pressed: bool,
    },
    /// Modifier state, which arrives alongside every key event.
    Modifiers {
        depressed: u32,
        latched: u32,
        locked: u32,
    },
    /// Keyboard focus arrived or left; modifier state is stale after a leave.
    FocusChanged(bool),
    PointerMoved {
        x: f64,
        y: f64,
    },
    PointerButton {
        button: u32,
        pressed: bool,
    },
    /// The compositor is ready for the next frame.
    Frame,
    /// The lock is gone and cannot be assumed to hold.
    Finished {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockError {
    /// The compositor refused the connection outright.
    Rejected(String),
    /// `session-lock` was denied — the usual cause is the greeter not running
    /// from the trusted binary directory the compositor checks.
    CapabilityDenied(String),
    /// A malformed request on our side.
    Protocol { code: String, message: String },
    /// A message arrived that makes no sense in the current phase.
    Unexpected(String),
    /// An operation was attempted that requires an engaged lock.
    NotEngaged,
}

impl fmt::Display for LockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected(reason) => write!(f, "SCP connection rejected: {reason}"),
            Self::CapabilityDenied(reason) => {
                write!(f, "the compositor denied '{SESSION_LOCK}': {reason}")
            }
            Self::Protocol { code, message } => write!(f, "SCP protocol error [{code}]: {message}"),
            Self::Unexpected(what) => write!(f, "unexpected SCP message: {what}"),
            Self::NotEngaged => write!(f, "the session lock is not engaged"),
        }
    }
}

impl std::error::Error for LockError {}

/// Messages to send, and events for the UI, produced by one inbound message.
#[derive(Debug, Default, Clone)]
pub struct LockStep {
    pub outbound: Vec<ClientMessage>,
    pub events: Vec<LockEvent>,
}

impl LockStep {
    fn nothing() -> Self {
        Self::default()
    }

    fn send(message: ClientMessage) -> Self {
        Self {
            outbound: vec![message],
            events: Vec::new(),
        }
    }

    fn event(event: LockEvent) -> Self {
        Self {
            outbound: Vec::new(),
            events: vec![event],
        }
    }
}

/// One lock surface, which always covers exactly one output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Surface {
    surface_id: SurfaceId,
    lock_surface_id: Option<LockSurfaceId>,
    output_id: Option<OutputId>,
}

/// Drives the greeter's side of the session lock.
#[derive(Debug)]
pub struct LockDriver {
    phase: LockPhase,
    /// Held across an unlock so the screen can be locked again when the user's
    /// session ends, without re-asking for the capability.
    capability_token: Option<Vec<u8>>,
    lock_id: Option<LockId>,
    surfaces: Vec<Surface>,
    next_surface_id: SurfaceId,
    next_frame_callback: u32,
    size: (i32, i32),
}

impl Default for LockDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl LockDriver {
    pub fn new() -> Self {
        Self {
            phase: LockPhase::Idle,
            capability_token: None,
            lock_id: None,
            surfaces: Vec::new(),
            next_surface_id: 1,
            next_frame_callback: 1,
            size: (0, 0),
        }
    }

    /// The opening message. `app_id` must match what the compositor derives from
    /// the process, or the connection is rejected.
    pub fn start(&mut self, app_id: String, pid: u32) -> ClientMessage {
        self.phase = LockPhase::Connecting;
        ClientMessage::Connect { app_id, pid }
    }

    pub const fn phase(&self) -> &LockPhase {
        &self.phase
    }

    pub const fn is_locked(&self) -> bool {
        matches!(self.phase, LockPhase::Locked)
    }

    /// Size of the primary lock surface, once the compositor has configured it.
    pub const fn size(&self) -> (i32, i32) {
        self.size
    }

    /// The surface the login UI draws into.
    fn primary_surface(&self) -> Option<SurfaceId> {
        self.surfaces.first().map(|surface| surface.surface_id)
    }

    /// Advance the handshake with one message from the compositor.
    pub fn handle(&mut self, message: CompositorMessage) -> Result<LockStep, LockError> {
        match message {
            CompositorMessage::Connected {
                capability_tokens, ..
            } => match capability_tokens.get(SESSION_LOCK) {
                // The stub coordinator grants only the default capability set at
                // connect, so this is normally the request path. Honoring a
                // token that did arrive keeps us correct if that changes.
                Some(token) => {
                    self.capability_token = Some(token.clone());
                    Ok(LockStep::send(self.engage()))
                }
                None => {
                    self.phase = LockPhase::RequestingCapability;
                    Ok(LockStep::send(ClientMessage::RequestCapability {
                        capability: SESSION_LOCK.to_string(),
                        justification: JUSTIFICATION.to_string(),
                    }))
                }
            },

            CompositorMessage::Rejected { reason } => {
                self.phase = LockPhase::Finished(reason.clone());
                Err(LockError::Rejected(reason))
            }

            CompositorMessage::CapabilityDecision {
                capability,
                granted,
                token,
                reason,
                ..
            } => {
                if capability != SESSION_LOCK {
                    return Ok(LockStep::nothing());
                }
                match (granted, token) {
                    (true, Some(token)) => {
                        self.capability_token = Some(token);
                        Ok(LockStep::send(self.engage()))
                    }
                    _ => {
                        let reason = reason.unwrap_or_else(|| "no reason given".to_string());
                        self.phase = LockPhase::Finished(reason.clone());
                        Err(LockError::CapabilityDenied(reason))
                    }
                }
            }

            CompositorMessage::SessionLockEngaged { lock_id } => {
                self.lock_id = Some(lock_id);
                self.phase = LockPhase::Presenting;
                // The desktop is already cut off at this point but nothing is
                // drawn, so cover the primary output immediately. Omitting the
                // output id lets the compositor resolve it, which is also the
                // only thing that works before any output is registered.
                Ok(LockStep {
                    outbound: self.create_surface(None),
                    events: Vec::new(),
                })
            }

            CompositorMessage::ConfigureLockSurface {
                lock_surface_id,
                serial,
                width,
                height,
            } => {
                self.adopt_lock_surface(lock_surface_id);

                let mut step = LockStep::send(ClientMessage::AckLockConfigure {
                    lock_surface_id,
                    serial,
                });
                // Only the primary surface backs the login UI; a secondary
                // output is covered but not drawn into.
                if self.surfaces.first().and_then(|s| s.lock_surface_id) == Some(lock_surface_id)
                    && self.size != (width, height)
                {
                    self.size = (width, height);
                    step.events.push(LockEvent::Resized { width, height });
                }
                Ok(step)
            }

            CompositorMessage::SessionLocked { lock_id } => {
                self.lock_id = Some(lock_id);
                self.phase = LockPhase::Locked;
                Ok(LockStep::event(LockEvent::Locked))
            }

            CompositorMessage::SessionLockFinished { reason } => {
                self.phase = LockPhase::Finished(reason.clone());
                self.lock_id = None;
                self.surfaces.clear();
                Ok(LockStep::event(LockEvent::Finished { reason }))
            }

            CompositorMessage::OutputAdded { output_id, .. } => {
                // A lock has to cover every output or the compositor will not
                // consider the session locked. Nothing is drawn on the new one,
                // but leaving it uncovered would leave a live strip of desktop.
                if self.lock_id.is_none()
                    || self
                        .surfaces
                        .iter()
                        .any(|surface| surface.output_id == Some(output_id))
                {
                    return Ok(LockStep::nothing());
                }
                Ok(LockStep {
                    outbound: self.create_surface(Some(output_id)),
                    events: Vec::new(),
                })
            }

            CompositorMessage::InputEvent { event, .. } => Ok(input_event(event)),

            CompositorMessage::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                ..
            } => Ok(LockStep::event(LockEvent::Modifiers {
                depressed: mods_depressed,
                latched: mods_latched,
                locked: mods_locked,
            })),

            CompositorMessage::FrameCallback { .. } => Ok(LockStep::event(LockEvent::Frame)),

            CompositorMessage::ProtocolError {
                code,
                message,
                fatal,
            } => {
                if fatal {
                    self.phase = LockPhase::Finished(message.clone());
                    return Err(LockError::Protocol { code, message });
                }
                tracing::warn!(%code, %message, "non-fatal SCP protocol error");
                Ok(LockStep::nothing())
            }

            // The keymap arrives with keyboard focus. The greeter decodes with
            // its own table (see `super::keys`), so there is nothing to do with
            // it; the descriptor is closed by the transport.
            CompositorMessage::KeymapFormat { .. }
            | CompositorMessage::RepeatInfo { .. }
            | CompositorMessage::OutputChanged { .. }
            | CompositorMessage::OutputRemoved { .. }
            | CompositorMessage::OutputGeometryChanged { .. }
            | CompositorMessage::OutputScaleChanged { .. }
            | CompositorMessage::OutputModeChanged { .. }
            | CompositorMessage::SurfaceEnterOutput { .. }
            | CompositorMessage::SurfaceLeaveOutput { .. }
            | CompositorMessage::BufferRelease { .. }
            | CompositorMessage::SessionLockStateChanged { .. } => Ok(LockStep::nothing()),

            other => Err(LockError::Unexpected(format!("{other:?}"))),
        }
    }

    /// Build the frame-submission messages for a rendered buffer.
    ///
    /// The returned [`Presentation::attach`] carries `buffer_fd` out of band —
    /// SCP only accepts a descriptor with `AttachBuffer`, and the field is not
    /// serialized.
    pub fn present(
        &mut self,
        buffer_fd: RawFd,
        width: i32,
        height: i32,
        stride: i32,
    ) -> Result<Presentation, LockError> {
        let surface_id = self.primary_surface().ok_or(LockError::NotEngaged)?;
        let callback_id = self.next_frame_callback;
        self.next_frame_callback = self.next_frame_callback.wrapping_add(1).max(1);

        Ok(Presentation {
            attach: ClientMessage::AttachBuffer {
                surface_id,
                buffer_fd,
                width,
                height,
                stride,
                format: super::buffer::FORMAT,
            },
            buffer_fd,
            rest: vec![
                ClientMessage::Damage {
                    surface_id,
                    x: 0,
                    y: 0,
                    width,
                    height,
                },
                ClientMessage::Commit {
                    surface_id,
                    frame_callback: Some(callback_id),
                },
            ],
        })
    }

    /// Release the lock after a successful authentication.
    ///
    /// Releasing the lock does not dispose of the surfaces underneath it — those
    /// were created with `CreateSurface` and are the client's to destroy. Left
    /// behind they would accumulate in the compositor across every
    /// lock/unlock cycle, so they go with the lock.
    pub fn unlock(&mut self) -> Result<Vec<ClientMessage>, LockError> {
        let lock_id = self.lock_id.take().ok_or(LockError::NotEngaged)?;
        self.phase = LockPhase::Released;
        self.size = (0, 0);

        // Unlock first: the compositor's lock bookkeeping refers to these
        // surfaces, so it is torn down before they are.
        let mut messages = vec![ClientMessage::UnlockSession { lock_id }];
        messages.extend(
            self.surfaces
                .drain(..)
                .map(|surface| ClientMessage::DestroySurface {
                    surface_id: surface.surface_id,
                }),
        );
        Ok(messages)
    }

    /// Lock the screen again, once the user's session has ended.
    ///
    /// The capability token survives an unlock, so this skips straight back to
    /// `LockSession` rather than repeating the whole handshake.
    ///
    /// Surface ids keep counting up rather than restarting: the compositor keys
    /// surfaces by the id the client picks, so reusing one it has not yet seen
    /// destroyed is a collision rather than a fresh surface.
    pub fn relock(&mut self) -> Result<ClientMessage, LockError> {
        if self.capability_token.is_none() {
            return Err(LockError::NotEngaged);
        }
        Ok(self.engage())
    }

    /// Emit `LockSession` with the token we hold.
    fn engage(&mut self) -> ClientMessage {
        self.phase = LockPhase::Engaging;
        ClientMessage::LockSession {
            capability_token: self
                .capability_token
                .clone()
                .expect("engage is only reached once a token has been recorded"),
        }
    }

    /// Allocate a surface and ask for a lock surface on `output_id`.
    fn create_surface(&mut self, output_id: Option<OutputId>) -> Vec<ClientMessage> {
        let surface_id = self.next_surface_id;
        self.next_surface_id += 1;
        self.surfaces.push(Surface {
            surface_id,
            lock_surface_id: None,
            output_id,
        });

        let lock_id = self
            .lock_id
            .expect("a surface is only created once the lock is engaged");
        vec![
            ClientMessage::CreateSurface { surface_id },
            ClientMessage::CreateLockSurface {
                surface_id,
                lock_id,
                output_id,
            },
        ]
    }

    /// Bind a configure to the surface still waiting for one.
    ///
    /// Surfaces are created one at a time and the compositor replies to each
    /// before the next is asked for, so the first unbound surface is always the
    /// one this configure belongs to. A re-configure of an already-bound surface
    /// (an output resize) needs no binding at all.
    fn adopt_lock_surface(&mut self, lock_surface_id: LockSurfaceId) {
        let already_bound = self
            .surfaces
            .iter()
            .any(|surface| surface.lock_surface_id == Some(lock_surface_id));
        if already_bound {
            return;
        }
        if let Some(surface) = self
            .surfaces
            .iter_mut()
            .find(|surface| surface.lock_surface_id.is_none())
        {
            surface.lock_surface_id = Some(lock_surface_id);
        }
    }
}

/// Frame submission, split by which message carries the descriptor.
#[derive(Debug, Clone)]
pub struct Presentation {
    /// Must be written with SCM_RIGHTS carrying [`Self::buffer_fd`].
    pub attach: ClientMessage,
    pub buffer_fd: RawFd,
    /// Damage and commit, written as ordinary frames.
    pub rest: Vec<ClientMessage>,
}

/// Project an SCP input event onto what the login UI understands.
fn input_event(event: InputEvent) -> LockStep {
    let event = match event {
        InputEvent::KeyboardKey { key, state, .. } => LockEvent::Key {
            keycode: key,
            pressed: matches!(state, KeyState::Pressed),
        },
        InputEvent::Modifiers {
            mods_depressed,
            mods_latched,
            mods_locked,
            ..
        } => LockEvent::Modifiers {
            depressed: mods_depressed,
            latched: mods_latched,
            locked: mods_locked,
        },
        InputEvent::KeyboardEnter { .. } => LockEvent::FocusChanged(true),
        InputEvent::KeyboardLeave { .. } => LockEvent::FocusChanged(false),
        InputEvent::PointerMotion { x, y, .. } | InputEvent::PointerEnter { x, y, .. } => {
            LockEvent::PointerMoved { x, y }
        }
        InputEvent::PointerButton { button, state, .. } => LockEvent::PointerButton {
            button,
            pressed: matches!(state, ButtonState::Pressed),
        },
        // Touch drives the same avatar and button hit-testing as the pointer.
        InputEvent::TouchDown { x, y, .. } => {
            return LockStep {
                outbound: Vec::new(),
                events: vec![
                    LockEvent::PointerMoved { x, y },
                    LockEvent::PointerButton {
                        button: BTN_LEFT,
                        pressed: true,
                    },
                ],
            };
        }
        InputEvent::TouchUp { .. } => LockEvent::PointerButton {
            button: BTN_LEFT,
            pressed: false,
        },
        InputEvent::TouchMotion { x, y, .. } => LockEvent::PointerMoved { x, y },
        _ => return LockStep::nothing(),
    };
    LockStep::event(event)
}

/// Left mouse button, evdev `BTN_LEFT`.
pub const BTN_LEFT: u32 = 0x110;

/// Full-surface damage, for callers that need the rectangle itself.
pub const fn full_damage(width: i32, height: i32) -> Rect {
    Rect {
        x: 0,
        y: 0,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn token() -> Vec<u8> {
        b"test-token".to_vec()
    }

    fn connected(with_lock_token: bool) -> CompositorMessage {
        let mut capability_tokens = HashMap::new();
        capability_tokens.insert("window-toplevel".to_string(), b"other".to_vec());
        if with_lock_token {
            capability_tokens.insert(SESSION_LOCK.to_string(), token());
        }
        CompositorMessage::Connected {
            session_id: 1,
            granted_capabilities: capability_tokens.keys().cloned().collect(),
            capability_tokens,
        }
    }

    fn granted() -> CompositorMessage {
        CompositorMessage::CapabilityDecision {
            capability: SESSION_LOCK.to_string(),
            granted: true,
            token: Some(token()),
            reason: None,
            needs_user_consent: false,
        }
    }

    /// Drive a driver all the way to `Locked`.
    fn locked_driver() -> LockDriver {
        let mut driver = LockDriver::new();
        driver.start("sol-logind".to_string(), 1);
        driver.handle(connected(false)).expect("request capability");
        driver.handle(granted()).expect("engage");
        driver
            .handle(CompositorMessage::SessionLockEngaged { lock_id: 7 })
            .expect("create surfaces");
        driver
            .handle(CompositorMessage::ConfigureLockSurface {
                lock_surface_id: 1,
                serial: 3,
                width: 1920,
                height: 1080,
            })
            .expect("ack configure");
        driver
            .handle(CompositorMessage::SessionLocked { lock_id: 7 })
            .expect("locked");
        driver
    }

    #[test]
    fn requests_the_capability_when_connect_does_not_grant_it() {
        let mut driver = LockDriver::new();
        driver.start("sol-logind".to_string(), 1);

        let step = driver.handle(connected(false)).expect("handle Connected");
        assert!(matches!(
            step.outbound.as_slice(),
            [ClientMessage::RequestCapability { capability, .. }] if capability == SESSION_LOCK
        ));
        assert_eq!(driver.phase(), &LockPhase::RequestingCapability);
    }

    #[test]
    fn skips_the_request_when_connect_already_granted_it() {
        let mut driver = LockDriver::new();
        driver.start("sol-logind".to_string(), 1);

        let step = driver.handle(connected(true)).expect("handle Connected");
        assert!(matches!(
            step.outbound.as_slice(),
            [ClientMessage::LockSession { capability_token }] if *capability_token == token()
        ));
        assert_eq!(driver.phase(), &LockPhase::Engaging);
    }

    #[test]
    fn engaging_covers_the_primary_output_before_anything_is_drawn() {
        let mut driver = LockDriver::new();
        driver.start("sol-logind".to_string(), 1);
        driver.handle(connected(true)).expect("engage");

        let step = driver
            .handle(CompositorMessage::SessionLockEngaged { lock_id: 7 })
            .expect("handle engaged");
        assert!(matches!(
            step.outbound.as_slice(),
            [
                ClientMessage::CreateSurface { surface_id: 1 },
                ClientMessage::CreateLockSurface {
                    surface_id: 1,
                    lock_id: 7,
                    output_id: None,
                },
            ]
        ));
        assert_eq!(driver.phase(), &LockPhase::Presenting);
    }

    #[test]
    fn configure_is_acknowledged_and_reports_the_size() {
        let mut driver = LockDriver::new();
        driver.start("sol-logind".to_string(), 1);
        driver.handle(connected(true)).expect("engage");
        driver
            .handle(CompositorMessage::SessionLockEngaged { lock_id: 7 })
            .expect("engaged");

        let step = driver
            .handle(CompositorMessage::ConfigureLockSurface {
                lock_surface_id: 4,
                serial: 11,
                width: 1920,
                height: 1080,
            })
            .expect("handle configure");

        assert!(matches!(
            step.outbound.as_slice(),
            [ClientMessage::AckLockConfigure {
                lock_surface_id: 4,
                serial: 11,
            }]
        ));
        assert_eq!(
            step.events,
            vec![LockEvent::Resized {
                width: 1920,
                height: 1080,
            }]
        );
        assert_eq!(driver.size(), (1920, 1080));
    }

    #[test]
    fn reaches_locked_and_can_present_a_frame() {
        let mut driver = locked_driver();
        assert!(driver.is_locked());

        let presentation = driver.present(9, 1920, 1080, 1920 * 4).expect("present");
        assert_eq!(presentation.buffer_fd, 9);
        assert!(matches!(
            presentation.attach,
            ClientMessage::AttachBuffer {
                surface_id: 1,
                buffer_fd: 9,
                width: 1920,
                height: 1080,
                stride: 7680,
                ..
            }
        ));
        assert!(matches!(
            presentation.rest.as_slice(),
            [
                ClientMessage::Damage { surface_id: 1, .. },
                ClientMessage::Commit {
                    surface_id: 1,
                    frame_callback: Some(_),
                },
            ]
        ));
    }

    #[test]
    fn frame_callback_ids_do_not_repeat() {
        let mut driver = locked_driver();
        let first = frame_callback_of(&driver.present(9, 8, 8, 32).expect("first frame"));
        let second = frame_callback_of(&driver.present(9, 8, 8, 32).expect("second frame"));
        assert_ne!(first, second);
    }

    fn frame_callback_of(presentation: &Presentation) -> u32 {
        presentation
            .rest
            .iter()
            .find_map(|message| match message {
                ClientMessage::Commit { frame_callback, .. } => *frame_callback,
                _ => None,
            })
            .expect("commit carries a frame callback")
    }

    #[test]
    fn presenting_before_the_lock_engages_is_refused() {
        let mut driver = LockDriver::new();
        driver.start("sol-logind".to_string(), 1);
        assert_eq!(
            driver.present(9, 8, 8, 32).err(),
            Some(LockError::NotEngaged)
        );
    }

    #[test]
    fn a_denied_capability_is_reported_with_its_reason() {
        let mut driver = LockDriver::new();
        driver.start("sol-logind".to_string(), 1);
        driver.handle(connected(false)).expect("request");

        let error = driver
            .handle(CompositorMessage::CapabilityDecision {
                capability: SESSION_LOCK.to_string(),
                granted: false,
                token: None,
                reason: Some("Reserved for sol-logind".to_string()),
                needs_user_consent: false,
            })
            .expect_err("denial is an error");
        assert_eq!(
            error,
            LockError::CapabilityDenied("Reserved for sol-logind".to_string())
        );
    }

    #[test]
    fn a_rejected_connection_is_reported_with_its_reason() {
        let mut driver = LockDriver::new();
        driver.start("sol-logind".to_string(), 1);
        let error = driver
            .handle(CompositorMessage::Rejected {
                reason: "App ID mismatch".to_string(),
            })
            .expect_err("rejection is an error");
        assert_eq!(error, LockError::Rejected("App ID mismatch".to_string()));
    }

    #[test]
    fn a_withdrawn_lock_is_surfaced_and_drops_the_lock_state() {
        let mut driver = locked_driver();
        let step = driver
            .handle(CompositorMessage::SessionLockFinished {
                reason: "another client holds the lock".to_string(),
            })
            .expect("finished is not an error");

        assert_eq!(
            step.events,
            vec![LockEvent::Finished {
                reason: "another client holds the lock".to_string(),
            }]
        );
        assert!(!driver.is_locked());
        assert_eq!(
            driver.present(9, 8, 8, 32).err(),
            Some(LockError::NotEngaged)
        );
    }

    #[test]
    fn a_new_output_gets_its_own_lock_surface() {
        let mut driver = locked_driver();
        let step = driver
            .handle(CompositorMessage::OutputAdded {
                output_id: 3,
                name: "DP-2".to_string(),
                description: "second display".to_string(),
                geometry: full_damage(1280, 720),
                physical_size: (0, 0),
                subpixel: sol_compositor::scp::protocol::SubpixelLayout::Unknown,
                transform: sol_compositor::scp::protocol::Transform::Normal,
                scale: 1,
                modes: Vec::new(),
                current_mode: 0,
            })
            .expect("handle OutputAdded");

        assert!(matches!(
            step.outbound.as_slice(),
            [
                ClientMessage::CreateSurface { surface_id: 2 },
                ClientMessage::CreateLockSurface {
                    surface_id: 2,
                    output_id: Some(3),
                    ..
                },
            ]
        ));
    }

    #[test]
    fn the_same_output_is_not_covered_twice() {
        let mut driver = locked_driver();
        let added = |output_id| CompositorMessage::OutputAdded {
            output_id,
            name: "DP-2".to_string(),
            description: "second display".to_string(),
            geometry: full_damage(1280, 720),
            physical_size: (0, 0),
            subpixel: sol_compositor::scp::protocol::SubpixelLayout::Unknown,
            transform: sol_compositor::scp::protocol::Transform::Normal,
            scale: 1,
            modes: Vec::new(),
            current_mode: 0,
        };
        driver.handle(added(3)).expect("first add");
        let step = driver.handle(added(3)).expect("duplicate add");
        assert!(step.outbound.is_empty());
    }

    #[test]
    fn unlock_releases_and_relock_reuses_the_token() {
        let mut driver = locked_driver();

        let released = driver.unlock().expect("unlock");
        assert!(
            matches!(
                released.as_slice(),
                [
                    ClientMessage::UnlockSession { lock_id: 7 },
                    ClientMessage::DestroySurface { surface_id: 1 },
                ]
            ),
            "the lock is released and its surface disposed of"
        );
        assert_eq!(driver.phase(), &LockPhase::Released);
        assert_eq!(driver.unlock().err(), Some(LockError::NotEngaged));

        // Locking again does not re-ask for the capability.
        assert!(matches!(
            driver.relock().expect("relock"),
            ClientMessage::LockSession { capability_token } if capability_token == token()
        ));
        assert_eq!(driver.phase(), &LockPhase::Engaging);
    }

    #[test]
    fn key_events_carry_keycode_and_direction() {
        let mut driver = locked_driver();
        let step = driver
            .handle(CompositorMessage::InputEvent {
                surface_id: 1,
                event: InputEvent::KeyboardKey {
                    serial: 1,
                    key: 38,
                    state: KeyState::Pressed,
                    time_ms: 0,
                },
            })
            .expect("handle key");
        assert_eq!(
            step.events,
            vec![LockEvent::Key {
                keycode: 38,
                pressed: true,
            }]
        );
    }

    #[test]
    fn a_touch_down_reads_as_a_pointer_press_at_the_same_point() {
        let mut driver = locked_driver();
        let step = driver
            .handle(CompositorMessage::InputEvent {
                surface_id: 1,
                event: InputEvent::TouchDown {
                    serial: 1,
                    touch_id: 0,
                    x: 12.0,
                    y: 34.0,
                    time_ms: 0,
                },
            })
            .expect("handle touch");
        assert_eq!(
            step.events,
            vec![
                LockEvent::PointerMoved { x: 12.0, y: 34.0 },
                LockEvent::PointerButton {
                    button: BTN_LEFT,
                    pressed: true,
                },
            ]
        );
    }

    #[test]
    fn a_fatal_protocol_error_ends_the_session_but_a_warning_does_not() {
        let mut driver = locked_driver();
        assert!(
            driver
                .handle(CompositorMessage::ProtocolError {
                    code: "invalid-request".to_string(),
                    message: "bad damage rectangle".to_string(),
                    fatal: false,
                })
                .expect("non-fatal error is survivable")
                .events
                .is_empty()
        );
        assert!(driver.is_locked());

        let error = driver
            .handle(CompositorMessage::ProtocolError {
                code: "event-queue-overflow".to_string(),
                message: "client fell behind".to_string(),
                fatal: true,
            })
            .expect_err("fatal error stops the client");
        assert!(matches!(error, LockError::Protocol { .. }));
    }
}
