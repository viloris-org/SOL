//! Application-process lifecycle contracts.

use std::error::Error;
use std::fmt;

/// The externally observable lifecycle state of one application process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppState {
    /// The process has been created but has not finished startup.
    Starting,
    /// The process is running and eligible to present windows or commands.
    Running,
    /// The process remains alive but is not active in the current session.
    Suspended,
    /// The process has terminated. This state is terminal for this instance.
    #[default]
    Stopped,
}

/// A successfully applied lifecycle transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleTransition {
    /// Startup completed and the process is running.
    Started,
    /// The running process entered the inactive session state.
    Suspended,
    /// The inactive process became active again.
    Resumed,
    /// The process terminated.
    Stopped,
}

/// An invalid lifecycle operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LifecycleError {
    /// State in which the operation was requested.
    pub state: AppState,
    /// Operation that cannot be applied from [`Self::state`].
    pub operation: LifecycleOperation,
}

impl fmt::Display for LifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot {} an application while it is {}",
            self.operation, self.state
        )
    }
}

impl Error for LifecycleError {}

/// One lifecycle operation subject to transition validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleOperation {
    /// Finish startup.
    Start,
    /// Enter the inactive session state.
    Suspend,
    /// Return from the inactive session state.
    Resume,
    /// Terminate the process.
    Stop,
}

impl fmt::Display for LifecycleOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Start => "start",
            Self::Suspend => "suspend",
            Self::Resume => "resume",
            Self::Stop => "stop",
        })
    }
}

impl fmt::Display for AppState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Suspended => "suspended",
            Self::Stopped => "stopped",
        })
    }
}

/// State machine enforcing the lifecycle boundary of one application process.
///
/// A new process starts in [`AppState::Starting`]. It may move to `Running`,
/// then between `Running` and `Suspended`, and may stop from any live state.
/// `Stopped` is terminal: launching again creates a new lifecycle instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppLifecycle {
    state: AppState,
}

impl Default for AppLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl AppLifecycle {
    /// Create the lifecycle for a newly launched application process.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: AppState::Starting,
        }
    }

    /// Return the current lifecycle state.
    #[must_use]
    pub const fn state(self) -> AppState {
        self.state
    }

    /// Mark startup complete.
    pub fn start(&mut self) -> Result<LifecycleTransition, LifecycleError> {
        self.transition(LifecycleOperation::Start)
    }

    /// Mark the process inactive in the current session.
    pub fn suspend(&mut self) -> Result<LifecycleTransition, LifecycleError> {
        self.transition(LifecycleOperation::Suspend)
    }

    /// Mark the suspended process active again.
    pub fn resume(&mut self) -> Result<LifecycleTransition, LifecycleError> {
        self.transition(LifecycleOperation::Resume)
    }

    /// Mark the process terminated.
    pub fn stop(&mut self) -> Result<LifecycleTransition, LifecycleError> {
        self.transition(LifecycleOperation::Stop)
    }

    fn transition(
        &mut self,
        operation: LifecycleOperation,
    ) -> Result<LifecycleTransition, LifecycleError> {
        let (next_state, transition) = match (self.state, operation) {
            (AppState::Starting, LifecycleOperation::Start) => {
                (AppState::Running, LifecycleTransition::Started)
            }
            (AppState::Running, LifecycleOperation::Suspend) => {
                (AppState::Suspended, LifecycleTransition::Suspended)
            }
            (AppState::Suspended, LifecycleOperation::Resume) => {
                (AppState::Running, LifecycleTransition::Resumed)
            }
            (
                AppState::Starting | AppState::Running | AppState::Suspended,
                LifecycleOperation::Stop,
            ) => (AppState::Stopped, LifecycleTransition::Stopped),
            _ => {
                return Err(LifecycleError {
                    state: self.state,
                    operation,
                });
            }
        };

        self.state = next_state;
        Ok(transition)
    }
}

#[cfg(test)]
mod tests {
    use super::{AppLifecycle, AppState, LifecycleError, LifecycleOperation, LifecycleTransition};

    #[test]
    fn lifecycle_follows_the_supported_session_path() {
        let mut lifecycle = AppLifecycle::new();

        assert_eq!(lifecycle.state(), AppState::Starting);
        assert_eq!(lifecycle.start(), Ok(LifecycleTransition::Started));
        assert_eq!(lifecycle.suspend(), Ok(LifecycleTransition::Suspended));
        assert_eq!(lifecycle.resume(), Ok(LifecycleTransition::Resumed));
        assert_eq!(lifecycle.stop(), Ok(LifecycleTransition::Stopped));
        assert_eq!(lifecycle.state(), AppState::Stopped);
    }

    #[test]
    fn lifecycle_rejects_skipping_startup_or_resuming_the_wrong_state() {
        let mut lifecycle = AppLifecycle::new();

        assert_eq!(
            lifecycle.suspend(),
            Err(LifecycleError {
                state: AppState::Starting,
                operation: LifecycleOperation::Suspend,
            })
        );
        assert_eq!(
            lifecycle.resume(),
            Err(LifecycleError {
                state: AppState::Starting,
                operation: LifecycleOperation::Resume,
            })
        );
    }

    #[test]
    fn stopped_processes_cannot_be_restarted() {
        let mut lifecycle = AppLifecycle::new();
        lifecycle.stop().expect("live process can stop");

        assert_eq!(
            lifecycle.start(),
            Err(LifecycleError {
                state: AppState::Stopped,
                operation: LifecycleOperation::Start,
            })
        );
    }
}
