//! Frame timing telemetry and the compositor's RT safety watchdog.

use std::{io, time::Duration};

use crate::{demote_current_thread, promote_current_thread};

/// Aggregated compositor frame and input latency telemetry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameTelemetry {
    pub presented_frames: u64,
    pub missed_vsyncs: u64,
    pub watchdog_downgrades: u64,
    pub total_frame_time: Duration,
    pub maximum_frame_time: Duration,
    pub input_latency_samples: u64,
    pub total_input_latency: Duration,
    pub maximum_input_latency: Duration,
}

/// Result of recording one presented frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameObservation {
    pub missed_vsync: bool,
    pub watchdog_downgraded: bool,
}

/// SCHED_FIFO guard and telemetry for one render/present event-loop thread.
pub struct FrameWatchdog {
    frame_budget: Duration,
    watchdog_budget: Duration,
    realtime_enabled: bool,
    permanently_downgraded: bool,
    pending_input_age: Option<Duration>,
    telemetry: FrameTelemetry,
}

impl FrameWatchdog {
    /// Build a watchdog from an output refresh rate expressed in millihertz.
    #[must_use]
    pub fn for_refresh_millihz(refresh_millihz: u32) -> Self {
        let refresh_millihz = refresh_millihz.max(1);
        let frame_budget = Duration::from_nanos(1_000_000_000_000 / u64::from(refresh_millihz));
        Self {
            frame_budget,
            watchdog_budget: frame_budget.mul_f64(1.5),
            realtime_enabled: false,
            permanently_downgraded: false,
            pending_input_age: None,
            telemetry: FrameTelemetry::default(),
        }
    }

    /// Elevate the calling render/present thread. Permission denial is
    /// returned to the caller so development and CI can explicitly degrade.
    pub fn enable_realtime(&mut self, priority: i32) -> io::Result<()> {
        promote_current_thread(priority)?;
        self.realtime_enabled = true;
        Ok(())
    }

    /// Record the age of the newest input event at the next presentation.
    pub fn note_input_age(&mut self, age: Duration) {
        self.pending_input_age = Some(age);
    }

    /// Record a frame and automatically downgrade a misbehaving RT thread.
    pub fn observe(&mut self, frame_time: Duration) -> io::Result<FrameObservation> {
        self.telemetry.presented_frames += 1;
        self.telemetry.total_frame_time += frame_time;
        self.telemetry.maximum_frame_time = self.telemetry.maximum_frame_time.max(frame_time);
        let missed_vsync = frame_time > self.frame_budget;
        if missed_vsync {
            self.telemetry.missed_vsyncs += 1;
        }
        if let Some(input_latency) = self.pending_input_age.take() {
            self.telemetry.input_latency_samples += 1;
            self.telemetry.total_input_latency += input_latency;
            self.telemetry.maximum_input_latency =
                self.telemetry.maximum_input_latency.max(input_latency);
        }

        let should_downgrade = frame_time > self.watchdog_budget
            && self.realtime_enabled
            && !self.permanently_downgraded;
        if should_downgrade {
            demote_current_thread()?;
            self.realtime_enabled = false;
            self.permanently_downgraded = true;
            self.telemetry.watchdog_downgrades += 1;
        }
        Ok(FrameObservation {
            missed_vsync,
            watchdog_downgraded: should_downgrade,
        })
    }

    #[must_use]
    pub const fn telemetry(&self) -> FrameTelemetry {
        self.telemetry
    }

    #[must_use]
    pub const fn frame_budget(&self) -> Duration {
        self.frame_budget
    }

    #[must_use]
    pub const fn is_permanently_downgraded(&self) -> bool {
        self.permanently_downgraded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_budget_for_common_refresh_rates() {
        let sixty = FrameWatchdog::for_refresh_millihz(60_000);
        let one_twenty = FrameWatchdog::for_refresh_millihz(120_000);
        assert_eq!(sixty.frame_budget().as_micros(), 16_666);
        assert_eq!(one_twenty.frame_budget().as_micros(), 8_333);
    }

    #[test]
    fn collects_frame_and_input_metrics_without_rt_privileges() {
        let mut watchdog = FrameWatchdog::for_refresh_millihz(60_000);
        watchdog.note_input_age(Duration::from_millis(4));
        let observation = watchdog.observe(Duration::from_millis(17)).unwrap();
        assert!(observation.missed_vsync);
        assert!(!observation.watchdog_downgraded);
        assert_eq!(watchdog.telemetry().presented_frames, 1);
        assert_eq!(watchdog.telemetry().input_latency_samples, 1);
    }
}
