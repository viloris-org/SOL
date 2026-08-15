//! Design tokens module.
//!
//! ## Single source of truth
//!
//! **sol-design** is the *only* crate allowed to define concrete visual
//! parameters (colors, spacing, radii, durations, motion curves, …). UI
//! components in `sol-ui` and first-party applications must reference these
//! tokens by named value — never hand-write a bare `#RRGGBB`, `8.0`, or
//! `217ms`. Type-safe wrapper types turn "wrong" usage into a compile error
//! instead of a style drift, so consistency is enforced by the type system
//! rather than by convention.
//!
//! (PRD §19 Design Tokens / §4.1 Consistency First.)
//!
//! This phase ships a *minimal* token set — enough to build the first shell
//! screen and dogfood surfaces. It is intentionally NOT exhaustive:
//! categories and named values are added per-component in Design Review.

pub mod color;
pub mod material;
pub mod motion;
pub mod radius;
pub mod spacing;
pub mod typography;

/// Wallpaper / window-clear fallback background.
/// Temporary stand-in so real v0.1 surfaces have a defined root fill.
pub const DEFAULT_BACKGROUND: color::Rgba = color::Rgba(0.11, 0.10, 0.13, 1.0);
