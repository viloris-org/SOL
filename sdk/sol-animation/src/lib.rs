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
//! - `SessionHandoff`: Authenticated login surface yielding to a ready desktop

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

    /// Create a critically damped Liquid Glass materialization animation.
    pub fn material() -> Self {
        Self::new(Motion::Material)
    }

    /// Create an anchored shared-container morph.
    pub fn morph() -> Self {
        Self::new(Motion::Morph)
    }

    /// Create a momentum-preserving direct-manipulation settle.
    pub fn rebound() -> Self {
        Self::new(Motion::Rebound)
    }

    /// Create a window animation.
    pub fn window() -> Self {
        Self::new(Motion::Window)
    }

    /// Create a workspace animation.
    pub fn workspace() -> Self {
        Self::new(Motion::Workspace)
    }

    /// Create an authenticated login-to-desktop handoff animation.
    pub fn session_handoff() -> Self {
        Self::new(Motion::SessionHandoff)
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

/// One continuously retargetable spring-driven scalar.
///
/// Unlike a keyframe animation, this value never restarts from a logical
/// endpoint. Retargeting keeps its live presentation value and velocity, which
/// makes rapid open/close reversals continuous.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpringValue {
    value: f32,
    target: f32,
    velocity: f32,
    spec: MotionSpec,
}

impl SpringValue {
    /// Construct a spring at a stable initial value.
    pub fn new(motion: Motion, value: f32) -> Self {
        Self {
            value,
            target: value,
            velocity: 0.0,
            spec: motion.spec(),
        }
    }

    /// Return the live presentation value.
    pub const fn value(self) -> f32 {
        self.value
    }

    /// Return the current destination.
    pub const fn target(self) -> f32 {
        self.target
    }

    /// Return velocity carried by the current presentation state.
    pub const fn velocity(self) -> f32 {
        self.velocity
    }

    /// Replace spring behavior without changing presentation state.
    pub fn set_motion(&mut self, motion: Motion) {
        self.spec = motion.spec();
    }

    /// Retarget from the current on-screen value while preserving velocity.
    pub fn retarget(&mut self, target: f32) {
        self.target = target;
    }

    /// Retarget while handing off a gesture's release velocity.
    pub fn retarget_with_velocity(&mut self, target: f32, velocity: f32) {
        self.target = target;
        self.velocity = velocity;
    }

    /// Let a gesture take direct 1:1 ownership of presentation state.
    pub fn take_over(&mut self, value: f32, velocity: f32) {
        self.value = value;
        self.target = value;
        self.velocity = velocity;
    }

    /// Snap to a value, clearing inherited velocity.
    pub fn snap_to(&mut self, value: f32) {
        self.value = value;
        self.target = value;
        self.velocity = 0.0;
    }

    /// Advance the spring by elapsed seconds and return the new presentation.
    ///
    /// Large frame gaps are bounded and internally sub-stepped so a suspended
    /// surface cannot explode numerically when it becomes visible again.
    pub fn step(&mut self, elapsed_seconds: f32) -> f32 {
        if self.is_settled() || elapsed_seconds <= 0.0 {
            return self.value;
        }
        let Some((frequency, damping_ratio)) = self.spec.spring else {
            self.snap_to(self.target);
            return self.value;
        };
        let elapsed = elapsed_seconds.min(0.05);
        let step_count = (elapsed / (1.0 / 120.0)).ceil().max(1.0) as u32;
        let delta = elapsed / step_count as f32;
        for _ in 0..step_count {
            let displacement = self.value - self.target;
            let acceleration = -frequency * frequency * displacement
                - 2.0 * damping_ratio * frequency * self.velocity;
            self.velocity += acceleration * delta;
            self.value += self.velocity * delta;
        }
        if (self.value - self.target).abs() < 0.001 && self.velocity.abs() < 0.01 {
            self.snap_to(self.target);
        }
        self.value
    }

    /// Return whether both displacement and velocity are visually at rest.
    pub fn is_settled(&self) -> bool {
        (self.value - self.target).abs() < 0.001 && self.velocity.abs() < 0.01
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
        assert!(matches!(
            AnimationEffect::material().motion,
            Motion::Material
        ));
        assert!(matches!(AnimationEffect::morph().motion, Motion::Morph));
        assert!(matches!(AnimationEffect::rebound().motion, Motion::Rebound));
        assert!(matches!(AnimationEffect::window().motion, Motion::Window));
        assert!(matches!(
            AnimationEffect::workspace().motion,
            Motion::Workspace
        ));
        assert!(matches!(
            AnimationEffect::session_handoff().motion,
            Motion::SessionHandoff
        ));
    }

    #[test]
    fn spring_retargets_from_live_value_and_preserves_velocity() {
        let mut spring = SpringValue::new(Motion::Morph, 0.0);
        spring.retarget(1.0);
        for _ in 0..8 {
            spring.step(1.0 / 60.0);
        }
        let live = spring.value();
        let velocity = spring.velocity();
        assert!(live > 0.0 && live < 1.0);
        assert!(velocity > 0.0);

        spring.retarget(0.0);
        assert_eq!(spring.value(), live);
        assert_eq!(spring.velocity(), velocity);
        spring.step(1.0 / 60.0);
        assert!(spring.value().is_finite());
    }

    #[test]
    fn pointer_release_rebound_can_overshoot_then_settle() {
        let mut spring = SpringValue::new(Motion::Rebound, 0.97);
        spring.retarget_with_velocity(1.0, 1.4);
        let mut overshot = false;
        for _ in 0..90 {
            overshot |= spring.step(1.0 / 60.0) > 1.0;
        }
        assert!(overshot);
        assert!((spring.value() - 1.0).abs() < 0.001);
    }

    #[test]
    fn gesture_takeover_is_one_to_one() {
        let mut spring = SpringValue::new(Motion::Morph, 0.0);
        spring.retarget(1.0);
        spring.step(1.0 / 60.0);
        spring.take_over(0.42, -0.8);
        assert_eq!(spring.value(), 0.42);
        assert_eq!(spring.target(), 0.42);
        assert_eq!(spring.velocity(), -0.8);
    }
}
