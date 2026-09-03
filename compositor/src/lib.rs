//! SOL Compositor - Native SCP protocol implementation.
//!
//! # Lints
//!
//! This crate carries its own lint table (see `Cargo.toml`) instead of
//! inheriting `[workspace.lints]`, because it is the one crate that must call
//! the kernel directly. Everything else about the workspace policy applies here;
//! the exception is `unsafe_code`, and it is written down rather than implied.

// `expect` in a test is a deliberate assertion, not an unhandled error: a
// fixture that cannot be built should fail the test loudly and immediately.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod drm_backend;
pub mod native_input;
pub mod scp;
