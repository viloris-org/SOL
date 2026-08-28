//! Per-session outbound event queues.
//!
//! SCP request handling is synchronous: a client sends a message and receives
//! its direct replies on the same socket. Compositor-initiated events — input,
//! frame callbacks, popup dismissal, clipboard offers — have no request to
//! answer, and often target a *different* client than the one that caused them.
//!
//! Each authenticated session therefore owns a [`SessionSink`]: a bounded queue
//! plus an eventfd. Any thread holding the compositor state can enqueue an event
//! for any session; the sink signals its eventfd, and that session's transport
//! thread wakes from `poll` and writes the frames out.
//!
//! A client that stops reading cannot be allowed to grow the compositor's memory
//! without bound, so the queue is capped. Overflow marks the sink instead of
//! dropping events silently, and the transport disconnects the offending client.

use crate::scp::{
    protocol::{CompositorMessage, SessionId},
    unix_socket,
};
use std::{
    collections::{HashMap, VecDeque},
    io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
    sync::{Arc, Mutex},
};

/// Events buffered for one client before it is treated as unresponsive.
pub const MAX_QUEUED_EVENTS: usize = 4096;

/// One compositor→client message, plus any descriptor it carries.
///
/// Descriptor-bearing messages declare their `fd` field `#[serde(skip)]`, so the
/// integer never appears in the JSON payload. SCM_RIGHTS is the only channel for
/// it, and [`OutboundEvent`] is what keeps the two halves together until the
/// transport can send them in one `sendmsg`.
#[derive(Debug)]
pub struct OutboundEvent {
    pub message: CompositorMessage,
    pub fd: Option<OwnedFd>,
}

impl OutboundEvent {
    pub const fn new(message: CompositorMessage) -> Self {
        Self { message, fd: None }
    }

    /// Attach a descriptor, taking ownership so it is closed if the event is
    /// dropped before it reaches the client.
    pub const fn with_fd(message: CompositorMessage, fd: OwnedFd) -> Self {
        Self {
            message,
            fd: Some(fd),
        }
    }

    /// Attach a raw descriptor the caller is handing over.
    ///
    /// # Safety
    ///
    /// `fd` must be an open descriptor that no one else will close.
    pub unsafe fn from_raw_fd(message: CompositorMessage, fd: RawFd) -> Self {
        // SAFETY: delegated to the caller by this function's contract.
        Self::with_fd(message, unsafe { OwnedFd::from_raw_fd(fd) })
    }
}

/// Why a queued event never reached its client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendError {
    /// No sink is registered for that session, or it has been closed.
    NoSuchSession,
    /// The client is not draining its queue.
    Overflowed,
}

#[derive(Debug, Default)]
struct SinkQueue {
    events: VecDeque<OutboundEvent>,
    overflowed: bool,
    closed: bool,
}

/// Outbound queue for a single client connection.
#[derive(Debug)]
pub struct SessionSink {
    queue: Mutex<SinkQueue>,
    wake: OwnedFd,
}

impl SessionSink {
    pub fn new() -> io::Result<Arc<Self>> {
        let wake = unix_socket::create_eventfd()?;
        // SAFETY: create_eventfd returned an owned, open descriptor.
        let wake = unsafe { OwnedFd::from_raw_fd(wake) };
        Ok(Arc::new(Self {
            queue: Mutex::new(SinkQueue::default()),
            wake,
        }))
    }

    /// Descriptor the transport thread polls alongside its client socket.
    pub fn wake_fd(&self) -> RawFd {
        self.wake.as_raw_fd()
    }

    /// Enqueue an event and wake the transport thread.
    pub fn push(&self, event: OutboundEvent) -> Result<(), SendError> {
        let mut queue = self.lock();

        if queue.closed {
            return Err(SendError::NoSuchSession);
        }
        if queue.overflowed {
            return Err(SendError::Overflowed);
        }
        if queue.events.len() >= MAX_QUEUED_EVENTS {
            queue.overflowed = true;
            queue.events.clear();
            drop(queue);
            // Wake the transport so it observes the overflow and disconnects.
            unix_socket::signal_eventfd(self.wake_fd());
            return Err(SendError::Overflowed);
        }

        queue.events.push_back(event);
        drop(queue);
        unix_socket::signal_eventfd(self.wake_fd());
        Ok(())
    }

    /// Take every pending event, leaving the queue empty.
    pub fn drain(&self) -> Vec<OutboundEvent> {
        let mut queue = self.lock();
        unix_socket::drain_eventfd(self.wake.as_raw_fd());
        queue.events.drain(..).collect()
    }

    /// Whether this client stopped draining its queue and must be disconnected.
    pub fn is_overflowed(&self) -> bool {
        self.lock().overflowed
    }

    /// Refuse further events. Queued descriptors are closed as the events drop.
    pub fn close(&self) {
        let mut queue = self.lock();
        queue.closed = true;
        queue.events.clear();
    }

    pub fn pending(&self) -> usize {
        self.lock().events.len()
    }

    /// A poisoned sink mutex only means some thread panicked mid-enqueue; the
    /// queue itself stays consistent, so recover rather than cascade the panic
    /// into an otherwise healthy compositor.
    fn lock(&self) -> std::sync::MutexGuard<'_, SinkQueue> {
        self.queue.lock().unwrap_or_else(|poisoned| {
            tracing::warn!("SCP session sink mutex was poisoned; recovering");
            poisoned.into_inner()
        })
    }
}

/// Routes compositor-initiated events to the sessions they belong to.
#[derive(Debug, Default)]
pub struct EventRouter {
    sinks: HashMap<SessionId, Arc<SessionSink>>,
}

impl EventRouter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, session_id: SessionId, sink: Arc<SessionSink>) {
        self.sinks.insert(session_id, sink);
    }

    /// Stop routing to a session and close its sink.
    pub fn unregister(&mut self, session_id: SessionId) -> Option<Arc<SessionSink>> {
        let sink = self.sinks.remove(&session_id)?;
        sink.close();
        Some(sink)
    }

    pub fn is_registered(&self, session_id: SessionId) -> bool {
        self.sinks.contains_key(&session_id)
    }

    pub fn send(&self, session_id: SessionId, message: CompositorMessage) -> Result<(), SendError> {
        self.send_event(session_id, OutboundEvent::new(message))
    }

    pub fn send_event(&self, session_id: SessionId, event: OutboundEvent) -> Result<(), SendError> {
        self.sinks
            .get(&session_id)
            .ok_or(SendError::NoSuchSession)?
            .push(event)
    }

    /// Deliver `messages` to their sessions, logging rather than failing on
    /// sessions that vanished — a disconnect racing an input event is routine.
    pub fn send_all(&self, messages: impl IntoIterator<Item = (SessionId, CompositorMessage)>) {
        for (session_id, message) in messages {
            self.send_logged(session_id, message);
        }
    }

    /// Send one message, logging delivery failures instead of propagating them.
    pub fn send_logged(&self, session_id: SessionId, message: CompositorMessage) {
        if let Err(error) = self.send(session_id, message) {
            tracing::debug!(?session_id, ?error, "dropped SCP event");
        }
    }

    /// Send `message` to every session except `excluded`.
    pub fn broadcast_except(&self, excluded: SessionId, message: &CompositorMessage) {
        for (&session_id, sink) in &self.sinks {
            if session_id == excluded {
                continue;
            }
            if let Err(error) = sink.push(OutboundEvent::new(message.clone())) {
                tracing::debug!(?session_id, ?error, "dropped broadcast SCP event");
            }
        }
    }

    pub fn session_ids(&self) -> impl Iterator<Item = SessionId> + '_ {
        self.sinks.keys().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error_event() -> OutboundEvent {
        OutboundEvent::new(CompositorMessage::SelectionCleared)
    }

    #[test]
    fn drains_queued_events_in_order() {
        let sink = SessionSink::new().expect("create sink");
        for index in 0..3 {
            sink.push(OutboundEvent::new(CompositorMessage::BufferRelease {
                buffer_id: index,
            }))
            .expect("push event");
        }
        assert_eq!(sink.pending(), 3);

        let drained = sink.drain();
        assert_eq!(drained.len(), 3);
        assert_eq!(sink.pending(), 0);
        for (index, event) in drained.iter().enumerate() {
            match event.message {
                CompositorMessage::BufferRelease { buffer_id } => {
                    assert_eq!(buffer_id as usize, index);
                }
                ref other => panic!("unexpected event: {other:?}"),
            }
        }
    }

    #[test]
    fn marks_overflow_instead_of_growing_without_bound() {
        let sink = SessionSink::new().expect("create sink");
        for _ in 0..MAX_QUEUED_EVENTS {
            sink.push(error_event()).expect("push within capacity");
        }

        assert_eq!(sink.push(error_event()), Err(SendError::Overflowed));
        assert!(sink.is_overflowed());
        // The backlog is released so an unresponsive client cannot pin memory
        // until the transport gets around to closing it.
        assert_eq!(sink.pending(), 0);
    }

    #[test]
    fn closed_sink_rejects_events() {
        let sink = SessionSink::new().expect("create sink");
        sink.close();
        assert_eq!(sink.push(error_event()), Err(SendError::NoSuchSession));
    }

    #[test]
    fn router_broadcasts_to_every_other_session() {
        let mut router = EventRouter::new();
        let first = SessionSink::new().expect("create sink");
        let second = SessionSink::new().expect("create sink");
        let third = SessionSink::new().expect("create sink");
        router.register(1, Arc::clone(&first));
        router.register(2, Arc::clone(&second));
        router.register(3, Arc::clone(&third));

        router.broadcast_except(2, &CompositorMessage::SelectionCleared);

        assert_eq!(first.pending(), 1);
        assert_eq!(second.pending(), 0);
        assert_eq!(third.pending(), 1);
    }

    #[test]
    fn unregister_closes_the_sink() {
        let mut router = EventRouter::new();
        let sink = SessionSink::new().expect("create sink");
        router.register(7, Arc::clone(&sink));

        assert!(router.is_registered(7));
        router.unregister(7).expect("sink was registered");
        assert!(!router.is_registered(7));

        assert_eq!(
            router.send(7, CompositorMessage::SelectionCleared),
            Err(SendError::NoSuchSession)
        );
        assert_eq!(sink.push(error_event()), Err(SendError::NoSuchSession));
    }
}
