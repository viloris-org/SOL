//! Named component geometry tokens.
//!
//! SolUI selects one of these roles instead of embedding a visual metric.

/// Semantic control geometry role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlMetric {
    /// The standard button minimum size.
    Button,
    /// The standard editable-field height.
    TextField,
    /// The standard tab height.
    Tab,
    /// The standard toolbar height.
    Toolbar,
}

/// Resolved logical component dimensions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MetricSpec {
    /// Minimum logical width.
    pub min_width: f32,
    /// Logical height.
    pub height: f32,
}

impl ControlMetric {
    /// Resolve the named role to its design-controlled geometry.
    pub const fn spec(self) -> MetricSpec {
        match self {
            Self::Button => MetricSpec {
                min_width: 100.0,
                height: 32.0,
            },
            Self::TextField => MetricSpec {
                min_width: 100.0,
                height: 32.0,
            },
            Self::Tab => MetricSpec {
                min_width: 0.0,
                height: 32.0,
            },
            Self::Toolbar => MetricSpec {
                min_width: 0.0,
                height: 40.0,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_control_metric_has_a_non_negative_geometry() {
        for metric in [
            ControlMetric::Button,
            ControlMetric::TextField,
            ControlMetric::Tab,
            ControlMetric::Toolbar,
        ] {
            let spec = metric.spec();
            assert!(spec.min_width >= 0.0);
            assert!(spec.height > 0.0);
        }
    }
}
