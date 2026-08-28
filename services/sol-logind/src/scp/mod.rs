//! The login screen's SOL Compositor Protocol client.
//!
//! `sol-logind` reaches the screen the same way every other SOL component does:
//! as an SCP client. It does not open KMS, and it does not run before the
//! compositor. What makes it different from `sol-shell` is the capability it
//! holds — [`SessionLock`], which grants a surface above every layer, exclusive
//! input, and immunity from capture, and which the compositor grants to nothing
//! else, not even the shell (see ADR-0028 and
//! `sol_compositor::scp::session_lock`).
//!
//! The pieces are split by what they need:
//!
//! - [`lock`] is the protocol, as a state machine over messages. No socket, no
//!   descriptors — which is what lets the whole handshake be tested against a
//!   real compositor state in-process.
//! - [`client`] is the transport: the Unix socket, frame boundaries, and the
//!   SCM_RIGHTS handoff that `AttachBuffer` needs.
//! - [`buffer`] is the shared memory the login UI rasterizes into.
//! - [`keys`] turns the XKB keycodes SCP delivers into characters.
//!
//! [`SessionLock`]: sol_compositor::scp::capability::Capability::SessionLock

pub mod buffer;
pub mod client;
pub mod keys;
pub mod lock;

pub use buffer::FrameBuffer;
pub use client::ScpClient;
pub use keys::{KeyInput, Modifiers};
pub use lock::{LockDriver, LockError, LockEvent, LockPhase, LockStep};
