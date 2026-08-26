//! Privileged scheduling primitives for SOL system components.
//!
//! Applications never call this crate directly. `sol-init` owns process and
//! cgroup policy, while the compositor, shell, and audio server only use the
//! narrow current-thread real-time API.

mod audio;
mod cgroup;
mod linux;
mod policy;
mod watchdog;

pub use audio::{AudioObservation, AudioTelemetry, AudioWatchdog};
pub use cgroup::{BuildContainment, CgroupHierarchy, SchedulingManager};
pub use linux::{ApplyFailure, ApplyReport, demote_current_thread, promote_current_thread};
pub use policy::{
    AUDIO_RT_PRIORITY, COMPOSITOR_RT_PRIORITY, CgroupProfile, CpuLimit, IoPriority, ProcessClass,
    ProcessPolicy, SHELL_RT_PRIORITY, cgroup_profiles,
};
pub use watchdog::{FrameObservation, FrameTelemetry, FrameWatchdog};
