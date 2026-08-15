//! Spacing tokens.
//!
//! Named scale used by layout components and padding helpers.
//! Components must request `Spacing::Md` rather than a bare `12.0`.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Spacing {
    Xs,
    Sm,
    Md,
    Lg,
    Xl,
}

impl Spacing {
    /// Physical pixels at the baseline 1.0 scale factor (before output
    /// scaling). Layout multiplies by `sol-graphics` surface scale.
    pub const fn px(self) -> f32 {
        match self {
            Spacing::Xs => 4.0,
            Spacing::Sm => 8.0,
            Spacing::Md => 12.0,
            Spacing::Lg => 20.0,
            Spacing::Xl => 32.0,
        }
    }
}
