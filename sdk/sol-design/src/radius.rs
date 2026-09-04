//! Corner radius tokens.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Radius {
    /// Sharp — no rounding (e.g. full-bleed media, dividers).
    None,
    /// Subtle — panels, inputs, buttons.
    Sm,
    /// Medium — cards, floating panels.
    Md,
    /// Large — sheets and compact glass containers.
    Lg,
    /// Extra large — prominent media overlays and roomy glass panels.
    Xl,
    /// Pill / fully rounded — chips, avatars.
    Full,
}

impl Radius {
    pub const fn px(self) -> f32 {
        match self {
            Radius::None => 0.0,
            Radius::Sm => 4.0,
            Radius::Md => 10.0,
            Radius::Lg => 20.0,
            Radius::Xl => 32.0,
            Radius::Full => 9999.0,
        }
    }
}
