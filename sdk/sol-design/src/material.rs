//! Material / elevation tokens.
//!
//! Elevation controls shadow + surface ordering so a panel vs a window vs a
//! menu get a consistent depth language. Components request an elevation
//! level; the renderer maps it to blur + shadow + surface-color mixing.
//!
//! v0.1 ships a minimal set — enough to distinguish base / panel / popup.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Elevation {
    /// Default window surface / app content.
    Base,
    /// Dock, top bar, sidebar — floats above content.
    Panel,
    /// Popover, menu, tooltip — floats above panels.
    Floating,
}

/// Render description resolved by `sol-graphics`.
#[derive(Debug, Clone, Copy)]
pub struct ShadowSpec {
    pub blur: f32,
    pub offset_y: f32,
    pub opacity: f32,
}

impl Elevation {
    pub fn shadow(self) -> ShadowSpec {
        match self {
            Elevation::Base => ShadowSpec {
                blur: 0.0,
                offset_y: 0.0,
                opacity: 0.0,
            },
            Elevation::Panel => ShadowSpec {
                blur: 12.0,
                offset_y: 2.0,
                opacity: 0.18,
            },
            Elevation::Floating => ShadowSpec {
                blur: 22.0,
                offset_y: 6.0,
                opacity: 0.22,
            },
        }
    }
}
