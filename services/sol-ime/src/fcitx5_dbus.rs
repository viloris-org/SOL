//! D-Bus transport for an fcitx5 input context.
//!
//! This adapter uses the public `org.fcitx.Fcitx.InputMethod1` / `InputContext1`
//! contract.  It creates one input context, requests client-side preedit and
//! candidate UI, forwards key presses, and translates fcitx signals into the
//! frontend-neutral events in [`crate::engine`].

use crate::engine::{EngineError, Fcitx5Event, Fcitx5Request, Fcitx5Transport};
use crate::preedit::Preedit;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedObjectPath;

const FCITX_SERVICE: &str = "org.fcitx.Fcitx5";
const INPUT_METHOD_PATH: &str = "/org/freedesktop/portal/inputmethod";
const INPUT_METHOD_INTERFACE: &str = "org.fcitx.Fcitx.InputMethod1";
const INPUT_CONTEXT_INTERFACE: &str = "org.fcitx.Fcitx.InputContext1";
const CLIENT_CAPABILITIES: u64 = (1 << 0) | (1 << 1) | (1 << 4) | (1 << 39);
const EVENT_SETTLE_TIMEOUT: Duration = Duration::from_millis(40);

type FormattedText = Vec<(String, i32)>;
type CandidatePairs = Vec<(String, String)>;
type ClientSideUi = (
    FormattedText,
    i32,
    FormattedText,
    FormattedText,
    CandidatePairs,
    i32,
    i32,
    bool,
    bool,
);

/// A live fcitx5 session-bus input-context transport.
///
/// Construct this once per focused SOL IME client.  The listener subscribes
/// before the context receives focus, avoiding the race where initial preedit
/// or candidate updates are lost.
pub struct Fcitx5DbusTransport {
    context: Proxy<'static>,
    events: Receiver<Fcitx5Event>,
}

impl std::fmt::Debug for Fcitx5DbusTransport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Fcitx5DbusTransport")
            .finish_non_exhaustive()
    }
}

impl Fcitx5DbusTransport {
    /// Connect to the user's session-bus fcitx5 service.
    ///
    /// # Errors
    ///
    /// Returns an error when fcitx5 is unavailable or rejects the input
    /// context setup.  Callers may fall back to [`crate::engine::NoopEngine`]
    /// without changing their frontend flow.
    pub fn connect(program: &str) -> Result<Self, EngineError> {
        let connection = Connection::session().map_err(dbus_error)?;
        let input_method = Proxy::new(
            &connection,
            FCITX_SERVICE,
            INPUT_METHOD_PATH,
            INPUT_METHOD_INTERFACE,
        )
        .map_err(dbus_error)?;
        let description = vec![(program.to_owned(), "sol-ime".to_owned())];
        let (path, _uuid): (OwnedObjectPath, Vec<u8>) = input_method
            .call("CreateInputContext", &description)
            .map_err(dbus_error)?;
        let context = Proxy::new_owned(
            connection,
            FCITX_SERVICE,
            path.as_str().to_owned(),
            INPUT_CONTEXT_INTERFACE,
        )
        .map_err(dbus_error)?;

        let signals = context.receive_all_signals().map_err(dbus_error)?;
        let (sender, events) = mpsc::channel();
        thread::Builder::new()
            .name("sol-ime-fcitx-signals".to_owned())
            .spawn(move || {
                for message in signals {
                    for event in decode_signal(&message) {
                        if sender.send(event).is_err() {
                            return;
                        }
                    }
                }
            })
            .map_err(|error| {
                EngineError::transport(format!("spawn fcitx signal listener: {error}"))
            })?;

        context
            .call::<_, _, ()>("SetSupportedCapability", &CLIENT_CAPABILITIES)
            .map_err(dbus_error)?;
        context
            .call::<_, _, ()>("SetCapability", &CLIENT_CAPABILITIES)
            .map_err(dbus_error)?;
        context
            .call::<_, _, ()>("FocusIn", &())
            .map_err(dbus_error)?;

        Ok(Self { context, events })
    }

    fn collect_events(&self) -> Result<Vec<Fcitx5Event>, EngineError> {
        let mut events = Vec::new();
        loop {
            match self.events.recv_timeout(EVENT_SETTLE_TIMEOUT) {
                Ok(event) => events.push(event),
                // fcitx commonly emits preedit and candidate updates as
                // separate adjacent signals.  Wait for one quiet interval so
                // one frontend result reflects the complete update batch.
                Err(mpsc::RecvTimeoutError::Timeout) => return Ok(events),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(EngineError::transport("fcitx signal listener disconnected"));
                }
            }
        }
    }

    fn process_key(&self, keyval: u32) -> Result<(), EngineError> {
        // `false` is an fcitx key press; keycode 0 is intentional because this
        // bridge receives already composed Unicode text rather than a physical
        // keyboard scan code.  Fcitx uses `keyval` for pinyin characters.
        let event = (keyval, 0_u32, 0_u32, false, 0_u32);
        self.context
            .call::<_, _, bool>("ProcessKeyEvent", &event)
            .map(|_| ())
            .map_err(dbus_error)
    }
}

impl Fcitx5Transport for Fcitx5DbusTransport {
    fn request(&mut self, request: Fcitx5Request) -> Result<Vec<Fcitx5Event>, EngineError> {
        match request {
            Fcitx5Request::TypeText(text) => {
                for character in text.chars() {
                    self.process_key(character.into())?;
                }
            }
            Fcitx5Request::SelectCandidate(index) => {
                let index = i32::try_from(index)
                    .map_err(|_| EngineError::InvalidEvent("candidate index exceeds i32"))?;
                self.context
                    .call::<_, _, ()>("SelectCandidate", &index)
                    .map_err(dbus_error)?;
            }
            Fcitx5Request::Reset => {
                self.context
                    .call::<_, _, ()>("Reset", &())
                    .map_err(dbus_error)?;
            }
        }
        self.collect_events()
    }
}

impl Drop for Fcitx5DbusTransport {
    fn drop(&mut self) {
        let _ = self.context.call::<_, _, ()>("FocusOut", &());
    }
}

fn decode_signal(message: &zbus::Message) -> Vec<Fcitx5Event> {
    let header = message.header();
    let Some(member) = header.member().map(|member| member.as_str()) else {
        return Vec::new();
    };
    match member {
        "CommitString" => message
            .body()
            .deserialize::<String>()
            .ok()
            .map(Fcitx5Event::Commit)
            .into_iter()
            .collect(),
        "UpdateFormattedPreedit" => {
            let Ok((segments, cursor)) = message.body().deserialize::<(FormattedText, i32)>()
            else {
                return Vec::new();
            };
            vec![Fcitx5Event::Preedit(preedit_from_segments(
                segments, cursor,
            ))]
        }
        "UpdateClientSideUI" => {
            let Ok((
                preedit,
                cursor,
                _aux_up,
                _aux_down,
                candidates,
                selected,
                _layout,
                _has_prev,
                _has_next,
            )) = message.body().deserialize::<ClientSideUi>()
            else {
                return Vec::new();
            };
            let values = candidates
                .into_iter()
                .map(|(_label, value)| value)
                .collect();
            let selected = usize::try_from(selected).ok();
            let preedit = preedit_from_segments(preedit, cursor);
            vec![
                Fcitx5Event::Preedit(preedit),
                Fcitx5Event::Candidates { values, selected },
            ]
        }
        "HidePreedit" | "HidePreeditText" => vec![Fcitx5Event::Clear],
        _ => Vec::new(),
    }
}

fn preedit_from_segments(segments: FormattedText, cursor: i32) -> Preedit {
    let text: String = segments.into_iter().map(|(text, _format)| text).collect();
    let cursor = usize::try_from(cursor).unwrap_or(0).min(text.len());
    let cursor = if cursor == text.len() {
        cursor
    } else {
        text.char_indices()
            .map(|(index, _)| index)
            .take_while(|index| *index <= cursor)
            .last()
            .unwrap_or(0)
    };
    Preedit {
        active: !text.is_empty(),
        text,
        cursor,
    }
}

fn dbus_error(error: impl std::fmt::Display) -> EngineError {
    EngineError::transport(format!("fcitx5 D-Bus: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{Fcitx5DbusTransport, preedit_from_segments};
    use crate::engine::{Fcitx5Request, Fcitx5Transport};

    #[test]
    fn preedit_cursor_never_splits_a_cjk_character() {
        let preedit = preedit_from_segments(vec![("山西".to_owned(), 0)], 4);
        assert_eq!(preedit.text, "山西");
        assert_eq!(preedit.cursor, 3);
    }

    #[test]
    #[ignore = "requires a running fcitx5 session bus service"]
    fn fcitx5_session_bus_smoke() {
        let mut transport = Fcitx5DbusTransport::connect("sol-ime-smoke")
            .expect("fcitx5 input context should connect");
        let events = transport
            .request(Fcitx5Request::TypeText("shan".to_owned()))
            .expect("fcitx5 should accept pinyin key events");

        assert!(
            !events.is_empty(),
            "fcitx5 did not emit an observable preedit or candidate update"
        );

        assert!(
            events.iter().any(|event| matches!(
                event,
                crate::engine::Fcitx5Event::Preedit(_)
                    | crate::engine::Fcitx5Event::Candidates { .. }
            )),
            "fcitx5 did not emit a frontend UI update"
        );
    }
}
