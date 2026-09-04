//! Motion tokens.
//!
//! Motion is a first-class interaction primitive (PRD §4.4 / §19), not
//! decoration. Animations are named by semantic intent, so the *same* action
//! across the shell and apps shares duration + curve from this single
//! table — preventing the "one off, two off" micro-timing drift that reads
//! as inconsistent.

/// Semantic motion intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    /// Instant / near-zero, only to preserve causality.
    None,
    /// Hover / state flips, immediate feedback.
    Fast,
    /// Panel, menu, popover appearance.
    Panel,
    /// Glass materialization: blur, edge response and scale arrive together.
    Material,
    /// Shape-preserving expansion from a compact trigger into a container.
    Morph,
    /// Pointer-release settle that may preserve a small amount of momentum.
    Rebound,
    /// Window move / resize / snap.
    Window,
    /// Workspace / overview paging.
    Workspace,
    /// Authenticated login surface handing the display to a ready desktop.
    SessionHandoff,
}

/// Duration + spring tuning emitted to the animation runtime.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionSpec {
    /// Milliseconds for the settling duration (0 for `None`/`Fast`).
    pub duration_ms: u32,
    /// Natural frequency / damping for spring; `None` => ease curve.
    pub spring: Option<(f32, f32)>,
}

impl Motion {
    pub fn spec(self) -> MotionSpec {
        match self {
            Motion::None => MotionSpec {
                duration_ms: 0,
                spring: None,
            },
            Motion::Fast => MotionSpec {
                duration_ms: 90,
                spring: None,
            },
            Motion::Panel => MotionSpec {
                duration_ms: 170,
                spring: None,
            },
            Motion::Material => MotionSpec {
                duration_ms: 220,
                spring: Some((20.0, 1.0)),
            },
            Motion::Morph => MotionSpec {
                duration_ms: 240,
                spring: Some((20.0, 1.0)),
            },
            Motion::Rebound => MotionSpec {
                duration_ms: 160,
                spring: Some((22.0, 0.82)),
            },
            Motion::Window => MotionSpec {
                duration_ms: 260,
                spring: Some((20.0, 0.85)),
            },
            Motion::Workspace => MotionSpec {
                duration_ms: 340,
                spring: Some((16.0, 0.82)),
            },
            Motion::SessionHandoff => MotionSpec {
                duration_ms: 260,
                // The handoff is not momentum-driven, so it must not bounce.
                spring: Some((20.0, 1.0)),
            },
        }
    }
}
