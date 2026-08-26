//! The single, reviewable policy table implementing ADR-0029 Phase 1.

/// A cgroup v2 CPU bandwidth limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuLimit {
    /// Runtime available during each period, in microseconds.
    pub quota_us: u32,
    /// Accounting period, in microseconds.
    pub period_us: u32,
}

impl CpuLimit {
    /// Render the kernel's `cpu.max` syntax.
    #[must_use]
    pub fn as_cgroup_value(self) -> String {
        format!("{} {}", self.quota_us, self.period_us)
    }
}

/// Linux I/O priority assigned to a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoPriority {
    /// Realtime I/O class, level 0 (trusted system components only).
    Realtime,
    /// Best-effort I/O class with a level from 0 (highest) to 7.
    BestEffort(u8),
    /// Idle I/O class.
    Idle,
}

/// Trusted scheduler classes. These are assigned by `sol-init`, never parsed
/// from application-controlled metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcessClass {
    Compositor,
    Audio,
    Shell,
    Network,
    System,
    Notification,
    Foreground,
    ForegroundGame,
    Background,
    Build,
}

impl ProcessClass {
    /// Return the cgroup leaf name for this class.
    #[must_use]
    pub const fn cgroup_name(self) -> &'static str {
        match self {
            Self::Compositor => "sol-compositor",
            Self::Audio => "sol-audio",
            Self::Shell => "sol-shell",
            Self::Network => "sol-network",
            Self::System | Self::Notification => "sol-system",
            Self::Foreground | Self::ForegroundGame => "sol-foreground",
            Self::Background => "sol-background",
            Self::Build => "sol-build",
        }
    }

    /// Return process-level protection and priority policy.
    #[must_use]
    pub const fn process_policy(self) -> ProcessPolicy {
        match self {
            Self::Compositor => ProcessPolicy::new(0, -900, IoPriority::Realtime),
            Self::Audio => ProcessPolicy::new(0, -900, IoPriority::Realtime),
            Self::Shell => ProcessPolicy::new(0, -800, IoPriority::BestEffort(0)),
            Self::Network => ProcessPolicy::new(-10, -800, IoPriority::BestEffort(1)),
            Self::System => ProcessPolicy::new(-10, -500, IoPriority::BestEffort(2)),
            Self::Notification => ProcessPolicy::new(-5, -500, IoPriority::BestEffort(2)),
            Self::Foreground => ProcessPolicy::new(0, 0, IoPriority::BestEffort(4)),
            Self::ForegroundGame => ProcessPolicy::new(-10, 0, IoPriority::BestEffort(4)),
            Self::Background => ProcessPolicy::new(10, 100, IoPriority::Idle),
            Self::Build => ProcessPolicy::new(10, 100, IoPriority::Idle),
        }
    }
}

/// Process-level controls applied after cgroup placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessPolicy {
    pub nice: i8,
    pub oom_score_adj: i16,
    pub io_priority: IoPriority,
}

impl ProcessPolicy {
    const fn new(nice: i8, oom_score_adj: i16, io_priority: IoPriority) -> Self {
        Self {
            nice,
            oom_score_adj,
            io_priority,
        }
    }
}

/// One leaf in the SOL cgroup v2 hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CgroupProfile {
    pub name: &'static str,
    pub cpu_weight: u16,
    pub cpu_limit: Option<CpuLimit>,
    pub io_weight: u16,
    pub memory_min_bytes: Option<u64>,
    /// ADR reservation target. cgroup v2 has no `cpu.min` controller; the
    /// implementation uses proportional `cpu.weight` protection instead.
    pub cpu_reservation_percent: Option<u8>,
}

/// The complete Phase 1 hierarchy from ADR-0029.
#[must_use]
pub const fn cgroup_profiles() -> &'static [CgroupProfile] {
    &CGROUP_PROFILES
}

const CGROUP_PROFILES: [CgroupProfile; 8] = [
    CgroupProfile {
        name: "sol-compositor",
        cpu_weight: 1000,
        cpu_limit: None,
        io_weight: 100,
        memory_min_bytes: None,
        cpu_reservation_percent: Some(10),
    },
    CgroupProfile {
        name: "sol-audio",
        cpu_weight: 1000,
        cpu_limit: None,
        io_weight: 100,
        memory_min_bytes: None,
        cpu_reservation_percent: Some(10),
    },
    CgroupProfile {
        name: "sol-shell",
        cpu_weight: 500,
        cpu_limit: None,
        io_weight: 100,
        memory_min_bytes: None,
        cpu_reservation_percent: Some(5),
    },
    CgroupProfile {
        name: "sol-network",
        cpu_weight: 1000,
        cpu_limit: None,
        io_weight: 200,
        memory_min_bytes: Some(64 * 1024 * 1024),
        cpu_reservation_percent: Some(5),
    },
    CgroupProfile {
        name: "sol-system",
        cpu_weight: 800,
        cpu_limit: None,
        io_weight: 100,
        memory_min_bytes: None,
        cpu_reservation_percent: Some(20),
    },
    CgroupProfile {
        name: "sol-foreground",
        cpu_weight: 1000,
        cpu_limit: None,
        io_weight: 100,
        memory_min_bytes: None,
        cpu_reservation_percent: None,
    },
    CgroupProfile {
        name: "sol-background",
        cpu_weight: 100,
        cpu_limit: Some(CpuLimit {
            quota_us: 20_000,
            period_us: 100_000,
        }),
        io_weight: 10,
        memory_min_bytes: None,
        cpu_reservation_percent: Some(5),
    },
    CgroupProfile {
        name: "sol-build",
        cpu_weight: 100,
        cpu_limit: Some(CpuLimit {
            quota_us: 80_000,
            period_us: 100_000,
        }),
        io_weight: 10,
        memory_min_bytes: None,
        cpu_reservation_percent: None,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_process_class_maps_to_a_declared_leaf() {
        let classes = [
            ProcessClass::Compositor,
            ProcessClass::Audio,
            ProcessClass::Shell,
            ProcessClass::Network,
            ProcessClass::System,
            ProcessClass::Notification,
            ProcessClass::Foreground,
            ProcessClass::ForegroundGame,
            ProcessClass::Background,
            ProcessClass::Build,
        ];
        for class in classes {
            assert!(
                cgroup_profiles()
                    .iter()
                    .any(|profile| profile.name == class.cgroup_name())
            );
        }
    }

    #[test]
    fn cpu_limits_use_kernel_syntax() {
        let background = cgroup_profiles()
            .iter()
            .find(|profile| profile.name == "sol-background")
            .unwrap();
        assert_eq!(
            background.cpu_limit.unwrap().as_cgroup_value(),
            "20000 100000"
        );
    }
}
