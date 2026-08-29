//! Renderer-neutral timing for the authenticated login-to-desktop handoff.

use std::time::Duration;

use sol_design::{accessibility::TokenMode, motion::Motion};

/// Visual values consumed by the login renderer for one handoff frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HandoffVisual {
    /// Opacity of labels, avatars, password controls, and actions.
    pub content_opacity: f32,
    /// Opacity of the panel material and its shadow.
    pub material_opacity: f32,
    /// Whether the last handoff frame has been reached.
    pub finished: bool,
}

impl Default for HandoffVisual {
    fn default() -> Self {
        Self {
            content_opacity: 1.0,
            material_opacity: 1.0,
            finished: false,
        }
    }
}

/// A semantic, time-based handoff that can be driven by any renderer.
#[derive(Debug, Clone, Copy)]
pub struct SessionHandoff {
    duration: Duration,
    reduced_motion: bool,
}

impl SessionHandoff {
    /// Resolve the handoff from the current accessibility-aware motion tokens.
    #[must_use]
    pub fn new(mode: TokenMode) -> Self {
        let spec = mode.motion_spec(Motion::SessionHandoff);
        Self {
            duration: Duration::from_millis(u64::from(spec.duration_ms)),
            reduced_motion: spec.spring.is_none(),
        }
    }

    /// Total time before the lock surface may be released.
    #[must_use]
    pub const fn duration(self) -> Duration {
        self.duration
    }

    /// Resolve the exact visual values at `elapsed`.
    #[must_use]
    pub fn visual_at(self, elapsed: Duration) -> HandoffVisual {
        if self.duration.is_zero() {
            return HandoffVisual {
                content_opacity: 0.0,
                material_opacity: 0.0,
                finished: true,
            };
        }

        let total_ms = self.duration.as_secs_f32() * 1_000.0;
        let elapsed_ms = elapsed.as_secs_f32() * 1_000.0;
        let finished = elapsed >= self.duration;

        if self.reduced_motion {
            let progress = ease_out((elapsed_ms / total_ms).clamp(0.0, 1.0));
            return HandoffVisual {
                content_opacity: 1.0 - progress,
                material_opacity: 1.0 - progress,
                finished,
            };
        }

        // Content leaves first; the material then dissolves from underneath it.
        // Both stages still end inside the single semantic 260ms handoff token.
        let content_end_ms = 160.0_f32.min(total_ms);
        let material_start_ms = 80.0_f32.min(total_ms);
        let material_duration_ms = (total_ms - material_start_ms).max(1.0);
        let content_progress = ease_out((elapsed_ms / content_end_ms).clamp(0.0, 1.0));
        let material_progress =
            ease_out(((elapsed_ms - material_start_ms) / material_duration_ms).clamp(0.0, 1.0));

        HandoffVisual {
            content_opacity: 1.0 - content_progress,
            material_opacity: 1.0 - material_progress,
            finished,
        }
    }
}

/// Strong UI ease-out: cubic-bezier(0.23, 1, 0.32, 1).
fn ease_out(progress: f32) -> f32 {
    cubic_bezier(progress, 0.23, 1.0, 0.32, 1.0)
}

/// Evaluate a CSS-style cubic Bézier by solving its x component first.
fn cubic_bezier(progress: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    if progress == 0.0 || progress == 1.0 {
        return progress;
    }

    let sample = |t: f32, a1: f32, a2: f32| {
        let inverse = 1.0 - t;
        3.0 * inverse * inverse * t * a1 + 3.0 * inverse * t * t * a2 + t * t * t
    };
    let mut low = 0.0;
    let mut high = 1.0;
    for _ in 0..16 {
        let midpoint = (low + high) / 2.0;
        if sample(midpoint, x1, x2) < progress {
            low = midpoint;
        } else {
            high = midpoint;
        }
    }
    sample((low + high) / 2.0, y1, y2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_handoff_stages_content_before_material() {
        let handoff = SessionHandoff::new(TokenMode::light());
        assert_eq!(handoff.duration(), Duration::from_millis(260));

        let early = handoff.visual_at(Duration::from_millis(80));
        assert!(early.content_opacity < 1.0);
        assert_eq!(early.material_opacity, 1.0);

        let middle = handoff.visual_at(Duration::from_millis(160));
        assert_eq!(middle.content_opacity, 0.0);
        assert!(middle.material_opacity > 0.0);
        assert!(!middle.finished);

        let final_frame = handoff.visual_at(handoff.duration());
        assert_eq!(final_frame.content_opacity, 0.0);
        assert_eq!(final_frame.material_opacity, 0.0);
        assert!(final_frame.finished);
    }

    #[test]
    fn reduced_motion_is_a_short_non_spatial_crossfade() {
        let handoff = SessionHandoff::new(TokenMode::light().reduced_motion());
        assert_eq!(handoff.duration(), Duration::from_millis(160));

        let middle = handoff.visual_at(Duration::from_millis(80));
        assert_eq!(middle.content_opacity, middle.material_opacity);
        assert!(middle.content_opacity > 0.0 && middle.content_opacity < 1.0);
    }
}
