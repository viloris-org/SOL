//! sol-app — Application framework for SolKit
//!
//! This crate provides a native Rust application framework for building
//! SOL-native apps. It handles application lifecycle, commands, and
//! window management.
//!
//! # Lifecycle
//!
//! Apps have these states:
//! - **Starting**: App is initialized
//! - **Running**: App is active and visible
//! - **Suspended**: App is in background (window not focused)
//! - **Stopped**: App is terminated
//!
//! # Commands
//!
//! Commands are the primary way apps interact with system services and
//! other apps. They are automatically exposed to menus, shortcuts, and
//! the command palette.

mod identity;
mod lifecycle;

use std::collections::HashMap;
use std::sync::Arc;

pub use identity::{APP_ID_MAX_LENGTH, AppId, AppIdError, AppIdentity, AppIdentityError};
pub use lifecycle::{
    AppLifecycle, AppState, LifecycleError, LifecycleOperation, LifecycleTransition,
};

/// An application instance.
#[derive(Debug)]
pub struct App {
    /// The app's identity.
    pub id: AppId,
    lifecycle: AppLifecycle,
    /// The app's window(s).
    pub windows: Vec<AppWindow>,
}

impl App {
    /// Create a new app with the given identity.
    pub fn new(id: AppId) -> Self {
        Self {
            id,
            lifecycle: AppLifecycle::new(),
            windows: Vec::new(),
        }
    }

    /// Return the current lifecycle state.
    #[must_use]
    pub const fn state(&self) -> AppState {
        self.lifecycle.state()
    }

    /// Finish application startup.
    pub fn start(&mut self) -> Result<LifecycleTransition, LifecycleError> {
        self.lifecycle.start()
    }

    /// Suspend the app after it loses activity in the current session.
    pub fn suspend(&mut self) -> Result<LifecycleTransition, LifecycleError> {
        self.lifecycle.suspend()
    }

    /// Resume a suspended app when it becomes active again.
    pub fn resume(&mut self) -> Result<LifecycleTransition, LifecycleError> {
        self.lifecycle.resume()
    }

    /// Stop this application process. A stopped instance cannot be restarted.
    pub fn stop(&mut self) -> Result<LifecycleTransition, LifecycleError> {
        self.lifecycle.stop()
    }

    /// Add a window to the app.
    pub fn add_window(&mut self, window: AppWindow) {
        self.windows.push(window);
    }
}

/// An app window.
#[derive(Debug)]
pub struct AppWindow {
    /// The window's title.
    pub title: String,
    /// The window's size (width, height).
    pub size: (f32, f32),
    /// The window's position (x, y).
    pub position: (f32, f32),
    /// Whether the window is maximized.
    pub maximized: bool,
    /// Whether the window is fullscreen.
    pub fullscreen: bool,
}

impl Default for AppWindow {
    fn default() -> Self {
        Self {
            title: String::new(),
            size: (800.0, 600.0),
            position: (100.0, 100.0),
            maximized: false,
            fullscreen: false,
        }
    }
}

impl AppWindow {
    /// Create a new window.
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            ..Default::default()
        }
    }

    /// Set the window size.
    pub fn with_size(mut self, width: f32, height: f32) -> Self {
        self.size = (width, height);
        self
    }

    /// Maximize the window.
    pub fn maximized(mut self) -> Self {
        self.maximized = true;
        self
    }

    /// Make the window fullscreen.
    pub fn fullscreen(mut self) -> Self {
        self.fullscreen = true;
        self
    }
}

/// Command registry.
#[derive(Default)]
pub struct CommandRegistry {
    /// Registered commands.
    commands: HashMap<String, Box<dyn Command>>,
}

impl CommandRegistry {
    /// Create a new command registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a command.
    pub fn register<T: Command + 'static>(&mut self, command: T) {
        let id = command.id().to_string();
        self.commands.insert(id, Box::new(command));
    }

    /// Get a command by ID.
    pub fn get(&self, id: &str) -> Option<&dyn Command> {
        self.commands.get(id).map(|b| b.as_ref())
    }

    /// Execute a command by ID.
    pub fn execute(&self, id: &str, args: CommandContext) -> CommandResult {
        self.commands
            .get(id)
            .map(|cmd| cmd.execute(args))
            .unwrap_or_else(|| CommandResult::failure(format!("Command not found: {}", id)))
    }
}

/// A command that can be executed by the app framework.
pub trait Command: Send + Sync {
    /// Get the command's unique identifier.
    fn id(&self) -> &'static str;

    /// Get the command's display name.
    fn title(&self) -> &'static str;

    /// Execute the command.
    fn execute(&self, ctx: CommandContext) -> CommandResult;
}

/// Context provided to commands when they execute.
#[derive(Debug)]
pub struct CommandContext {
    /// The current app state.
    pub app: Arc<std::sync::Mutex<AppState>>,
    /// Additional arguments passed to the command.
    pub args: Vec<String>,
}

impl Default for CommandContext {
    fn default() -> Self {
        Self {
            app: Arc::new(std::sync::Mutex::new(AppState::default())),
            args: Vec::new(),
        }
    }
}

impl CommandContext {
    /// Create a new command context.
    pub fn new(app: Arc<std::sync::Mutex<AppState>>) -> Self {
        Self {
            app,
            args: Vec::new(),
        }
    }

    /// Add an argument to the command.
    pub fn with_arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }
}

/// Result of a command execution.
#[derive(Debug)]
pub struct CommandResult {
    /// Whether the command succeeded.
    pub success: bool,
    /// Error message if the command failed.
    pub error: Option<String>,
    /// Data produced by the command.
    pub data: Option<String>,
}

impl CommandResult {
    /// Create a successful result.
    pub fn success(data: Option<String>) -> Self {
        Self {
            success: true,
            error: None,
            data,
        }
    }

    /// Create a failure result.
    pub fn failure(error: String) -> Self {
        Self {
            success: false,
            error: Some(error),
            data: None,
        }
    }
}

/// Application entry point.
pub trait AppTrait: Sized {
    /// Create a new app instance.
    fn new(id: AppId) -> Self;

    /// Called when the app starts.
    fn on_start(&mut self) {}

    /// Called when the app suspends.
    fn on_suspend(&mut self) {}

    /// Called when the app resumes.
    fn on_resume(&mut self) {}

    /// Called when the app should stop.
    fn on_stop(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_state_defaults_to_stopped() {
        let state = AppState::default();
        assert!(matches!(state, AppState::Stopped));
    }

    #[test]
    fn app_can_start_and_stop() {
        let id = AppId::parse("org.example.test").expect("test ID should parse");
        let mut app = App::new(id);
        app.start().expect("app should start");
        assert!(matches!(app.state(), AppState::Running));
        app.stop().expect("app should stop");
        assert!(matches!(app.state(), AppState::Stopped));
    }

    #[test]
    fn app_window_defaults() {
        let window = AppWindow::default();
        assert_eq!(window.title, "");
        assert_eq!(window.size, (800.0, 600.0));
        assert!(!window.maximized);
        assert!(!window.fullscreen);
    }

    #[test]
    fn app_window_can_be_configured() {
        let window = AppWindow::new("Test")
            .with_size(1024.0, 768.0)
            .maximized();
        assert_eq!(window.title, "Test");
        assert_eq!(window.size, (1024.0, 768.0));
        assert!(window.maximized);
    }

    #[test]
    fn command_registry_can_register_commands() {
        let mut registry = CommandRegistry::new();
        registry.register(TestCommand);
        assert!(registry.get("test").is_some());
    }

    #[test]
    fn command_registry_can_execute_commands() {
        let mut registry = CommandRegistry::new();
        registry.register(TestCommand);
        let result = registry.execute("test", CommandContext::default());
        assert!(result.success);
    }

    #[test]
    fn unknown_command_returns_failure() {
        let registry = CommandRegistry::new();
        let result = registry.execute("unknown", CommandContext::default());
        assert!(!result.success);
    }

    struct TestCommand;

    impl Command for TestCommand {
        fn id(&self) -> &'static str {
            "test"
        }

        fn title(&self) -> &'static str {
            "Test Command"
        }

        fn execute(&self, _ctx: CommandContext) -> CommandResult {
            CommandResult::success(None)
        }
    }
}
