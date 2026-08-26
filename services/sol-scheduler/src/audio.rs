//! Audio DSP deadline telemetry from ADR-0029.

use std::{io, time::Duration};

use crate::{AUDIO_RT_PRIORITY, promote_current_thread};

/// Aggregated execution-time data for an audio DSP thread.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AudioTelemetry {
    pub cycles: u64,
    pub budget_warnings: u64,
    pub buffer_overruns: u64,
    pub total_execution_time: Duration,
    pub maximum_execution_time: Duration,
}

/// Result of one DSP cycle observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioObservation {
    pub exceeded_half_period: bool,
    pub exceeded_buffer_period: bool,
}

/// Budget monitor for a `PipeWire` or `sol-audio` DSP/mixing thread.
pub struct AudioWatchdog {
    buffer_period: Duration,
    warning_budget: Duration,
    telemetry: AudioTelemetry,
}

impl AudioWatchdog {
    /// Create a watchdog for a PCM buffer size and sample rate.
    ///
    /// # Errors
    ///
    /// Returns `InvalidInput` when either value is zero.
    pub fn new(buffer_samples: u32, sample_rate_hz: u32) -> io::Result<Self> {
        if buffer_samples == 0 || sample_rate_hz == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "audio buffer samples and sample rate must be non-zero",
            ));
        }
        let period_nanos = u64::from(buffer_samples) * 1_000_000_000 / u64::from(sample_rate_hz);
        let buffer_period = Duration::from_nanos(period_nanos);
        Ok(Self {
            buffer_period,
            warning_budget: buffer_period / 2,
            telemetry: AudioTelemetry::default(),
        })
    }

    /// Elevate only the calling DSP/mixing thread to FIFO priority 10.
    ///
    /// # Errors
    ///
    /// Returns the operating-system error when the caller lacks real-time
    /// scheduling permission.
    pub fn enable_realtime(&self) -> io::Result<()> {
        promote_current_thread(AUDIO_RT_PRIORITY)
    }

    /// Record one DSP execution interval and update warning/overrun counts.
    #[must_use]
    pub fn observe(&mut self, execution_time: Duration) -> AudioObservation {
        self.telemetry.cycles += 1;
        self.telemetry.total_execution_time += execution_time;
        self.telemetry.maximum_execution_time =
            self.telemetry.maximum_execution_time.max(execution_time);
        let exceeded_half_period = execution_time > self.warning_budget;
        let exceeded_buffer_period = execution_time > self.buffer_period;
        if exceeded_half_period {
            self.telemetry.budget_warnings += 1;
        }
        if exceeded_buffer_period {
            self.telemetry.buffer_overruns += 1;
        }
        AudioObservation {
            exceeded_half_period,
            exceeded_buffer_period,
        }
    }

    #[must_use]
    pub const fn buffer_period(&self) -> Duration {
        self.buffer_period
    }

    #[must_use]
    pub const fn warning_budget(&self) -> Duration {
        self.warning_budget
    }

    #[must_use]
    pub const fn telemetry(&self) -> AudioTelemetry {
        self.telemetry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_48khz_buffer_period_and_half_period_warning() -> io::Result<()> {
        let mut watchdog = AudioWatchdog::new(256, 48_000)?;
        assert_eq!(watchdog.buffer_period().as_micros(), 5_333);
        assert_eq!(watchdog.warning_budget().as_micros(), 2_666);
        let observation = watchdog.observe(Duration::from_millis(3));
        assert!(observation.exceeded_half_period);
        assert!(!observation.exceeded_buffer_period);
        assert_eq!(watchdog.telemetry().budget_warnings, 1);
        Ok(())
    }

    #[test]
    fn rejects_zero_sized_audio_timeline() {
        assert!(matches!(
            AudioWatchdog::new(0, 48_000),
            Err(error) if error.kind() == io::ErrorKind::InvalidInput
        ));
    }
}
