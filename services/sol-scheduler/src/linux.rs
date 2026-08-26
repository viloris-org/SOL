//! Small safe Linux scheduling adapter used by the privileged session manager.

use std::{
    fs, io,
    path::Path,
    process::{Command, Stdio},
};

use thread_priority::{
    NormalThreadSchedulePolicy, RealtimeThreadSchedulePolicy, ThreadPriority, ThreadPriorityValue,
    ThreadSchedulePolicy, set_thread_priority_and_policy, thread_native_id,
};

use crate::{IoPriority, ProcessClass};

/// One independently reported policy application failure.
#[derive(Debug)]
pub struct ApplyFailure {
    pub control: &'static str,
    pub error: io::Error,
}

/// Result of applying all controls. A partial failure does not prevent the
/// remaining independent protections from being attempted.
#[derive(Debug, Default)]
pub struct ApplyReport {
    pub failures: Vec<ApplyFailure>,
}

impl ApplyReport {
    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.failures.is_empty()
    }

    pub(crate) fn capture(&mut self, control: &'static str, result: io::Result<()>) {
        if let Err(error) = result {
            self.failures.push(ApplyFailure { control, error });
        }
    }
}

/// Elevate only the calling Linux thread to `SCHED_FIFO`.
///
/// # Errors
///
/// Returns `InvalidInput` for priorities outside 1–99, or the operating
/// system error when the caller lacks real-time scheduling permission.
pub fn promote_current_thread(priority: i32) -> io::Result<()> {
    if !(1..=99).contains(&priority) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "SCHED_FIFO priority must be between 1 and 99",
        ));
    }
    let raw_priority = u8::try_from(priority)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let value = ThreadPriorityValue::try_from(raw_priority)
        .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?;
    set_thread_priority_and_policy(
        thread_native_id(),
        ThreadPriority::Crossplatform(value),
        ThreadSchedulePolicy::Realtime(RealtimeThreadSchedulePolicy::Fifo),
    )
    .map_err(io::Error::other)
}

/// Return the calling Linux thread to the normal CFS scheduler.
///
/// # Errors
///
/// Returns the operating system error if the calling thread's policy cannot
/// be changed.
pub fn demote_current_thread() -> io::Result<()> {
    set_thread_priority_and_policy(
        thread_native_id(),
        ThreadPriority::Crossplatform(
            ThreadPriorityValue::try_from(50)
                .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?,
        ),
        ThreadSchedulePolicy::Normal(NormalThreadSchedulePolicy::Other),
    )
    .map_err(io::Error::other)
}

pub fn apply_process_controls(proc_root: &Path, pid: u32, class: ProcessClass) -> ApplyReport {
    let policy = class.process_policy();
    let mut report = ApplyReport::default();
    report.capture("nice", set_process_nice(pid, policy.nice));
    report.capture(
        "oom_score_adj",
        set_oom_score_adj(proc_root, pid, policy.oom_score_adj),
    );
    report.capture("io_priority", set_io_priority(pid, policy.io_priority));
    report
}

fn set_process_nice(pid: u32, nice: i8) -> io::Result<()> {
    let status = Command::new("renice")
        .args(["-n", &nice.to_string(), "-p", &pid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "renice exited with status {status}"
        )))
    }
}

fn set_oom_score_adj(proc_root: &Path, pid: u32, score: i16) -> io::Result<()> {
    if !(-1000..=1000).contains(&score) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "oom_score_adj must be between -1000 and 1000",
        ));
    }
    fs::write(
        proc_root.join(pid.to_string()).join("oom_score_adj"),
        score.to_string(),
    )
}

fn set_io_priority(pid: u32, priority: IoPriority) -> io::Result<()> {
    let mut command = Command::new("ionice");
    match priority {
        IoPriority::Realtime => {
            command.args(["-c", "1", "-n", "0"]);
        }
        IoPriority::BestEffort(level) if level <= 7 => {
            command.args(["-c", "2", "-n", &level.to_string()]);
        }
        IoPriority::BestEffort(_) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "best-effort I/O level must be between 0 and 7",
            ));
        }
        IoPriority::Idle => {
            command.args(["-c", "3"]);
        }
    }
    let status = command
        .args(["-p", &pid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "ionice exited with status {status}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn realtime_priority_is_validated_before_syscall() {
        assert!(matches!(
            promote_current_thread(0),
            Err(error) if error.kind() == io::ErrorKind::InvalidInput
        ));
        assert!(matches!(
            promote_current_thread(100),
            Err(error) if error.kind() == io::ErrorKind::InvalidInput
        ));
    }
}
