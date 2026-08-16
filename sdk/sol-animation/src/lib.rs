//! sol-animation — Animation engine for SolKit
//!
//! This crate provides semantic animation specifications driven by `sol-design`
//! motion tokens. It defines the contract between the compositor's animation
//! runtime and UI components, enabling interruptible, gesture-driven animations.
//!
//! # Architecture
//!
//! ```text
//! Animation driver → MotionSpec → Property interpolation
//!                           ↓
//!               sol-design Motion tokens (duration, spring)
//! ```
//!
//! # Motion Tiers (PRD §14)
//!
//! - `Control`: Instant state changes (hover, focus)
//! - `Panel`: Menus, popovers, dock
//! - `Window`: Move, resize, snap
//! - `Workspace`: Pager, overview transitions

use sol_design::motion::{Motion, MotionSpec};

/// An animation driver that executes and manages animations.
///
/// The driver handles:
/// - Spring physics simulation
/// - Gesture progress → animation progress mapping
/// - Animation interruption and reversal
/// - Velocity-based animations
pub trait AnimationDriver {
    /// Start or update an animation with the given motion spec.
    fn animate(&mut self, spec: MotionSpec);
    
    /// Interrupt the current animation and reverse it.
    fn reverse(&mut self);
    
    /// Stop all current animations.
    fn stop(&mut self);
    
    /// Check if an animation is currently running.
    fn is_animating(&self) -> bool;
}

/// Animation context for components.
///
/// Provides the current animation progress and allows setting
/// the motion spec for component animations.
pub struct AnimationContext {
    /// Current progress of the animation (0.0..=1.0).
    pub progress: f32,
    /// Whether the animation is running.
    pub is_active: bool,
}

impl Default for AnimationContext {
    fn default() -> Self {
        Self {
            progress: 0.0,
            is_active: false,
        }
    }
}

impl AnimationContext {
    /// Create a new animation context.
    pub fn new() -> Self {
        Self::default()
    }
}

/// A semantic animation effect that uses motion tokens.
///
/// Components use this to request animations with semantic intent.
#[derive(Debug, Clone, Copy)]
pub struct AnimationEffect {
    /// The semantic motion type.
    pub motion: Motion,
    /// Optional override for the duration.
    pub duration_ms: Option<u32>,
}

impl AnimationEffect {
    /// Create a new animation effect with the default spec for the motion.
    pub fn new(motion: Motion) -> Self {
        Self {
            motion,
            duration_ms: None,
        }
    }

    /// Get the complete motion specification.
    pub fn spec(&self) -> MotionSpec {
        let base = self.motion.spec();
        MotionSpec {
            duration_ms: self.duration_ms.unwrap_or(base.duration_ms),
            spring: base.spring,
        }
    }

    /// Create a control-level animation (instant/fly).
    pub fn control() -> Self {
        Self::new(Motion::None)
    }

    /// Create a fast animation for state changes.
    pub fn fast() -> Self {
        Self::new(Motion::Fast)
    }

    /// Create a panel animation.
    pub fn panel() -> Self {
        Self::new(Motion::Panel)
    }

    /// Create a window animation.
    pub fn window() -> Self {
        Self::new(Motion::Window)
    }

    /// Create a workspace animation.
    pub fn workspace() -> Self {
        Self::new(Motion::Workspace)
    }
}

/// An interruptible animation sequence.
///
/// Allows animations to be interrupted by gestures or other inputs,
/// with seamless transition to the new animation.
pub struct InterruptibleAnimation {
    /// Whether this animation can be interrupted.
    pub interruptible: bool,
    /// The current motion spec.
    pub current: MotionSpec,
    /// Target progress value (for spring animations).
    pub target_progress: f32,
    /// Current velocity of the animation.
    pub velocity: f32,
}

impl InterruptibleAnimation {
    /// Create a new interruptible animation.
    pub fn new(motion: Motion) -> Self {
        let spec = motion.spec();
        Self {
            interruptible: true,
            current: spec,
            target_progress: 1.0,
            velocity: 0.0,
        }
    }

    /// Update the animation with new gesture progress.
    pub fn update_with_progress(&mut self, progress: f32) {
        self.target_progress = progress;
    }

    /// Set the animation velocity (e.g., from a flick gesture).
    pub fn set_velocity(&mut self, velocity: f32) {
        self.velocity = velocity;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motion_none_has_zero_duration() {
        let spec = Motion::None.spec();
        assert_eq!(spec.duration_ms, 0);
        assert!(spec.spring.is_none());
    }

    #[test]
    fn motion_fast_has_quick_duration() {
        let spec = Motion::Fast.spec();
        assert_eq!(spec.duration_ms, 90);
        assert!(spec.spring.is_none());
    }

    #[test]
    fn motion_window_has_spring() {
        let spec = Motion::Window.spec();
        assert!(spec.duration_ms > 0);
        assert!(spec.spring.is_some());
    }

    #[test]
    fn animation_effect_spec_uses_override() {
        let effect = AnimationEffect {
            motion: Motion::Fast,
            duration_ms: Some(200),
        };
        let spec = effect.spec();
        assert_eq!(spec.duration_ms, 200);
    }

    #[test]
    fn animation_context_default() {
        let ctx = AnimationContext::default();
        assert!(!ctx.is_active);
        assert_eq!(ctx.progress, 0.0);
    }

    #[test]
    fn interruptible_animation_can_be_created() {
        let anim = InterruptibleAnimation::new(Motion::Panel);
        assert!(anim.interruptible);
        assert!(anim.current.spring.is_some() || anim.current.duration_ms > 0);
    }

    #[test]
    fn animation_effect_factory_methods() {
        assert!(matches!(AnimationEffect::control().motion, Motion::None));
        assert!(matches!(AnimationEffect::fast().motion, Motion::Fast));
        assert!(matches!(AnimationEffect::panel().motion, Motion::Panel));
        assert!(matches!(AnimationEffect::window().motion, Motion::Window));
        assert!(matches!(AnimationEffect::workspace().motion, Motion::Workspace));
    }
}
