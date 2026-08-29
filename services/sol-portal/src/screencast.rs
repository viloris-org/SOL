//! Authorized screen-cast session lifecycle.
//!
//! XDG portal, compositor capture, and media-transport adapters consume this
//! state machine. Capture production is deliberately separate from PipeWire:
//! only a compositor-produced [`SafeCaptureFeed`] may be published, and neither
//! stage can start before the shared permission layer has produced a matching
//! [`PortalAuthorization`].

use crate::{PortalAuthorization, PortalRequest};
use sol_system::AppId;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::num::NonZeroU64;

/// Stable identifier owned by the portal process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScreenCastSessionId(NonZeroU64);

impl ScreenCastSessionId {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Source categories exposed by XDG ScreenCast selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenCastSource {
    Monitor,
    Window,
}

/// Cursor transport requested for a capture stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorMode {
    Hidden,
    Embedded,
    Metadata,
}

/// One compositor-produced feed whose protected pixels have already been
/// replaced.
///
/// This is the trust boundary between capture composition and a transport such
/// as PipeWire. Implementors of [`CaptureProducer`] must source screenshots,
/// recording, sharing, remote desktop, previews, and machine-vision consumers
/// from the same protected-content-aware compositor path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafeCaptureFeed {
    pub feed_id: u64,
    pub source: ScreenCastSource,
    pub size: (u32, u32),
}

/// One PipeWire-compatible stream description returned by a capture backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenCastStream {
    pub node_id: u32,
    /// The safe compositor feed this transport node publishes.
    pub feed_id: u64,
    pub source: ScreenCastSource,
    pub size: (u32, u32),
}

/// Externally visible lifecycle of one authorized session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenCastState {
    Created,
    SourcesSelected,
    Streaming,
    Closed,
}

/// Snapshot suitable for a portal D-Bus adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenCastSession {
    pub id: ScreenCastSessionId,
    pub caller: AppId,
    pub authorization_request_id: u64,
    pub state: ScreenCastState,
    pub sources: Vec<ScreenCastSource>,
    pub cursor_mode: Option<CursorMode>,
    pub streams: Vec<ScreenCastStream>,
}

/// Protected-content-aware compositor capture boundary.
pub trait CaptureProducer {
    fn start(
        &mut self,
        session: ScreenCastSessionId,
        caller: &AppId,
        sources: &[ScreenCastSource],
        cursor_mode: CursorMode,
    ) -> Result<Vec<SafeCaptureFeed>, String>;

    fn stop(&mut self, session: ScreenCastSessionId) -> Result<(), String>;
}

/// Transport boundary for already-safe capture feeds.
///
/// PipeWire belongs here: it exports buffers and controls client visibility,
/// but it never decides which compositor surfaces are safe to capture.
pub trait StreamTransport {
    fn publish(
        &mut self,
        session: ScreenCastSessionId,
        caller: &AppId,
        feeds: &[SafeCaptureFeed],
    ) -> Result<Vec<ScreenCastStream>, String>;

    fn unpublish(&mut self, session: ScreenCastSessionId) -> Result<(), String>;
}

/// Invalid lifecycle or adapter result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenCastError {
    WrongAuthorization,
    UnknownSession(ScreenCastSessionId),
    InvalidState {
        expected: ScreenCastState,
        actual: ScreenCastState,
    },
    EmptySources,
    DuplicateSource(ScreenCastSource),
    InvalidStream(&'static str),
    Backend(String),
    SessionIdExhausted,
}

impl fmt::Display for ScreenCastError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongAuthorization => {
                formatter.write_str("authorization is not for screen capture")
            }
            Self::UnknownSession(id) => {
                write!(formatter, "unknown screen-cast session {}", id.get())
            }
            Self::InvalidState { expected, actual } => {
                write!(
                    formatter,
                    "screen-cast state is {actual:?}, expected {expected:?}"
                )
            }
            Self::EmptySources => formatter.write_str("screen-cast source selection is empty"),
            Self::DuplicateSource(source) => {
                write!(formatter, "duplicate screen-cast source {source:?}")
            }
            Self::InvalidStream(message) => {
                write!(formatter, "invalid screen-cast stream: {message}")
            }
            Self::Backend(message) => write!(formatter, "screen-cast backend: {message}"),
            Self::SessionIdExhausted => formatter.write_str("screen-cast session IDs exhausted"),
        }
    }
}

impl Error for ScreenCastError {}

/// Portal-owned session coordinator.
pub struct ScreenCastManager<P, T> {
    producer: P,
    transport: T,
    next_id: u64,
    sessions: BTreeMap<ScreenCastSessionId, ScreenCastSession>,
}

impl<P: CaptureProducer, T: StreamTransport> ScreenCastManager<P, T> {
    #[must_use]
    pub fn new(producer: P, transport: T) -> Self {
        Self {
            producer,
            transport,
            next_id: 1,
            sessions: BTreeMap::new(),
        }
    }

    pub fn create(
        &mut self,
        authorization: PortalAuthorization,
    ) -> Result<ScreenCastSessionId, ScreenCastError> {
        if authorization.request() != PortalRequest::ScreenCapture {
            return Err(ScreenCastError::WrongAuthorization);
        }
        let id = NonZeroU64::new(self.next_id)
            .map(ScreenCastSessionId)
            .ok_or(ScreenCastError::SessionIdExhausted)?;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(ScreenCastError::SessionIdExhausted)?;
        self.sessions.insert(
            id,
            ScreenCastSession {
                id,
                caller: authorization.caller().clone(),
                authorization_request_id: authorization.request_id(),
                state: ScreenCastState::Created,
                sources: Vec::new(),
                cursor_mode: None,
                streams: Vec::new(),
            },
        );
        Ok(id)
    }

    pub fn select_sources(
        &mut self,
        id: ScreenCastSessionId,
        sources: Vec<ScreenCastSource>,
        cursor_mode: CursorMode,
    ) -> Result<(), ScreenCastError> {
        if sources.is_empty() {
            return Err(ScreenCastError::EmptySources);
        }
        for (index, source) in sources.iter().enumerate() {
            if sources[index + 1..].contains(source) {
                return Err(ScreenCastError::DuplicateSource(*source));
            }
        }
        let session = self.session_mut(id)?;
        require_state(session, ScreenCastState::Created)?;
        session.sources = sources;
        session.cursor_mode = Some(cursor_mode);
        session.state = ScreenCastState::SourcesSelected;
        Ok(())
    }

    pub fn start(
        &mut self,
        id: ScreenCastSessionId,
    ) -> Result<Vec<ScreenCastStream>, ScreenCastError> {
        let session = self.session(id)?.clone();
        require_state(&session, ScreenCastState::SourcesSelected)?;
        let cursor_mode = session.cursor_mode.ok_or(ScreenCastError::InvalidState {
            expected: ScreenCastState::SourcesSelected,
            actual: session.state,
        })?;
        let feeds = self
            .producer
            .start(id, &session.caller, &session.sources, cursor_mode)
            .map_err(ScreenCastError::Backend)?;
        if let Err(error) = validate_feeds(&feeds, &session.sources) {
            let _ = self.producer.stop(id);
            return Err(error);
        }
        let streams = match self.transport.publish(id, &session.caller, &feeds) {
            Ok(streams) => streams,
            Err(error) => {
                let _ = self.producer.stop(id);
                return Err(ScreenCastError::Backend(error));
            }
        };
        if let Err(error) = validate_streams(&streams, &feeds) {
            let _ = self.transport.unpublish(id);
            let _ = self.producer.stop(id);
            return Err(error);
        }
        let retained = self.session_mut(id)?;
        retained.streams.clone_from(&streams);
        retained.state = ScreenCastState::Streaming;
        Ok(streams)
    }

    pub fn close(&mut self, id: ScreenCastSessionId) -> Result<(), ScreenCastError> {
        let state = self.session(id)?.state;
        if state == ScreenCastState::Closed {
            return Ok(());
        }
        if state == ScreenCastState::Streaming {
            let transport_result = self.transport.unpublish(id);
            let producer_result = self.producer.stop(id);
            if let Err(error) = transport_result {
                return Err(ScreenCastError::Backend(error));
            }
            producer_result.map_err(ScreenCastError::Backend)?;
        }
        let session = self.session_mut(id)?;
        session.state = ScreenCastState::Closed;
        session.streams.clear();
        Ok(())
    }

    pub fn session(&self, id: ScreenCastSessionId) -> Result<&ScreenCastSession, ScreenCastError> {
        self.sessions
            .get(&id)
            .ok_or(ScreenCastError::UnknownSession(id))
    }

    fn session_mut(
        &mut self,
        id: ScreenCastSessionId,
    ) -> Result<&mut ScreenCastSession, ScreenCastError> {
        self.sessions
            .get_mut(&id)
            .ok_or(ScreenCastError::UnknownSession(id))
    }
}

fn require_state(
    session: &ScreenCastSession,
    expected: ScreenCastState,
) -> Result<(), ScreenCastError> {
    if session.state == expected {
        Ok(())
    } else {
        Err(ScreenCastError::InvalidState {
            expected,
            actual: session.state,
        })
    }
}

fn validate_streams(
    streams: &[ScreenCastStream],
    feeds: &[SafeCaptureFeed],
) -> Result<(), ScreenCastError> {
    if streams.is_empty() {
        return Err(ScreenCastError::InvalidStream(
            "backend returned no streams",
        ));
    }
    let mut node_ids = std::collections::BTreeSet::new();
    let mut published_feed_ids = std::collections::BTreeSet::new();
    for stream in streams {
        if stream.node_id == 0 || !node_ids.insert(stream.node_id) {
            return Err(ScreenCastError::InvalidStream(
                "node IDs must be unique and non-zero",
            ));
        }
        if stream.size.0 == 0 || stream.size.1 == 0 {
            return Err(ScreenCastError::InvalidStream(
                "stream dimensions must be non-zero",
            ));
        }
        if !published_feed_ids.insert(stream.feed_id) {
            return Err(ScreenCastError::InvalidStream(
                "a safe capture feed was published more than once",
            ));
        }
        if !feeds.iter().any(|feed| {
            feed.feed_id == stream.feed_id
                && feed.source == stream.source
                && feed.size == stream.size
        }) {
            return Err(ScreenCastError::InvalidStream(
                "stream does not describe a safe capture feed",
            ));
        }
    }
    if streams.len() != feeds.len() {
        return Err(ScreenCastError::InvalidStream(
            "transport did not publish every safe capture feed exactly once",
        ));
    }
    Ok(())
}

fn validate_feeds(
    feeds: &[SafeCaptureFeed],
    sources: &[ScreenCastSource],
) -> Result<(), ScreenCastError> {
    if feeds.is_empty() {
        return Err(ScreenCastError::InvalidStream(
            "capture producer returned no safe feeds",
        ));
    }
    let mut feed_ids = std::collections::BTreeSet::new();
    for feed in feeds {
        if feed.feed_id == 0 || !feed_ids.insert(feed.feed_id) {
            return Err(ScreenCastError::InvalidStream(
                "safe feed IDs must be unique and non-zero",
            ));
        }
        if feed.size.0 == 0 || feed.size.1 == 0 {
            return Err(ScreenCastError::InvalidStream(
                "safe feed dimensions must be non-zero",
            ));
        }
        if !sources.contains(&feed.source) {
            return Err(ScreenCastError::InvalidStream(
                "safe feed source was not selected",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PortalOutcome, PortalService};
    use sol_system::{
        ActionResult, DefaultDenyPolicy, MemoryActionAuditStore, PermissionGrant, PermissionKey,
        PermissionStore, SystemActionService, SystemCapability,
    };

    #[derive(Default)]
    struct AllowCapture;

    impl PermissionStore for AllowCapture {
        fn get(&self, key: &PermissionKey) -> ActionResult<Option<PermissionGrant>> {
            Ok((matches!(
                key.capability,
                SystemCapability::ScreenCapture | SystemCapability::OpenDocuments
            ))
            .then_some(PermissionGrant::Allow))
        }
        fn set(&self, _: PermissionKey, _: PermissionGrant) -> ActionResult<()> {
            Ok(())
        }
        fn revoke(&self, _: &PermissionKey) -> ActionResult<bool> {
            Ok(false)
        }
    }

    #[derive(Default)]
    struct FixtureProducer {
        stopped: Vec<ScreenCastSessionId>,
    }

    impl CaptureProducer for FixtureProducer {
        fn start(
            &mut self,
            _: ScreenCastSessionId,
            _: &AppId,
            sources: &[ScreenCastSource],
            _: CursorMode,
        ) -> Result<Vec<SafeCaptureFeed>, String> {
            Ok(sources
                .iter()
                .enumerate()
                .map(|(index, source)| SafeCaptureFeed {
                    feed_id: index as u64 + 1,
                    source: *source,
                    size: (1920, 1080),
                })
                .collect())
        }
        fn stop(&mut self, session: ScreenCastSessionId) -> Result<(), String> {
            self.stopped.push(session);
            Ok(())
        }
    }

    #[derive(Default)]
    struct FixtureTransport {
        unpublished: Vec<ScreenCastSessionId>,
        fail_publish: bool,
        wrong_feed: bool,
    }

    impl StreamTransport for FixtureTransport {
        fn publish(
            &mut self,
            _: ScreenCastSessionId,
            _: &AppId,
            feeds: &[SafeCaptureFeed],
        ) -> Result<Vec<ScreenCastStream>, String> {
            if self.fail_publish {
                return Err("transport unavailable".to_owned());
            }
            let mut streams: Vec<_> = feeds
                .iter()
                .enumerate()
                .map(|(index, feed)| ScreenCastStream {
                    node_id: index as u32 + 42,
                    feed_id: feed.feed_id,
                    source: feed.source,
                    size: feed.size,
                })
                .collect();
            if self.wrong_feed
                && let Some(stream) = streams.first_mut()
            {
                stream.feed_id = u64::MAX;
            }
            Ok(streams)
        }

        fn unpublish(&mut self, session: ScreenCastSessionId) -> Result<(), String> {
            self.unpublished.push(session);
            Ok(())
        }
    }

    fn authorization(request: PortalRequest) -> PortalAuthorization {
        let actions = SystemActionService::new(
            DefaultDenyPolicy,
            AllowCapture,
            MemoryActionAuditStore::default(),
        );
        let portal = PortalService::new(actions);
        let outcome = portal
            .request(AppId::parse("org.sol.capture-test").unwrap(), request)
            .unwrap();
        let PortalOutcome::Authorized(authorization) = outcome else {
            panic!("fixture request should be authorized");
        };
        authorization
    }

    #[test]
    fn authorized_session_follows_select_start_close_lifecycle() {
        let mut manager =
            ScreenCastManager::new(FixtureProducer::default(), FixtureTransport::default());
        let id = manager
            .create(authorization(PortalRequest::ScreenCapture))
            .unwrap();
        manager
            .select_sources(id, vec![ScreenCastSource::Monitor], CursorMode::Metadata)
            .unwrap();
        let streams = manager.start(id).unwrap();
        assert_eq!(streams[0].node_id, 42);
        assert_eq!(
            manager.session(id).unwrap().state,
            ScreenCastState::Streaming
        );
        manager.close(id).unwrap();
        assert_eq!(manager.session(id).unwrap().state, ScreenCastState::Closed);
    }

    #[test]
    fn wrong_authorization_and_invalid_order_are_rejected() {
        let mut manager =
            ScreenCastManager::new(FixtureProducer::default(), FixtureTransport::default());
        assert_eq!(
            manager.create(authorization(PortalRequest::OpenDocument {
                uri: "file:///tmp/report".to_owned(),
            })),
            Err(ScreenCastError::WrongAuthorization)
        );
        let id = manager
            .create(authorization(PortalRequest::ScreenCapture))
            .unwrap();
        assert!(matches!(
            manager.start(id),
            Err(ScreenCastError::InvalidState { .. })
        ));
        assert_eq!(
            manager.select_sources(
                id,
                vec![ScreenCastSource::Monitor, ScreenCastSource::Monitor],
                CursorMode::Embedded,
            ),
            Err(ScreenCastError::DuplicateSource(ScreenCastSource::Monitor))
        );
    }

    #[test]
    fn a_transport_failure_stops_the_safe_capture_producer() {
        let mut manager = ScreenCastManager::new(
            FixtureProducer::default(),
            FixtureTransport {
                fail_publish: true,
                ..FixtureTransport::default()
            },
        );
        let id = manager
            .create(authorization(PortalRequest::ScreenCapture))
            .unwrap();
        manager
            .select_sources(id, vec![ScreenCastSource::Monitor], CursorMode::Hidden)
            .unwrap();

        assert_eq!(
            manager.start(id),
            Err(ScreenCastError::Backend("transport unavailable".to_owned()))
        );
        assert_eq!(manager.producer.stopped, vec![id]);
        assert_eq!(
            manager.session(id).unwrap().state,
            ScreenCastState::SourcesSelected
        );
    }

    #[test]
    fn a_transport_cannot_substitute_an_unverified_feed() {
        let mut manager = ScreenCastManager::new(
            FixtureProducer::default(),
            FixtureTransport {
                wrong_feed: true,
                ..FixtureTransport::default()
            },
        );
        let id = manager
            .create(authorization(PortalRequest::ScreenCapture))
            .unwrap();
        manager
            .select_sources(id, vec![ScreenCastSource::Monitor], CursorMode::Hidden)
            .unwrap();

        assert_eq!(
            manager.start(id),
            Err(ScreenCastError::InvalidStream(
                "stream does not describe a safe capture feed"
            ))
        );
        assert_eq!(manager.transport.unpublished, vec![id]);
        assert_eq!(manager.producer.stopped, vec![id]);
        assert_eq!(
            manager.session(id).unwrap().state,
            ScreenCastState::SourcesSelected
        );
    }
}
