use std::{cell::RefCell, env, rc::Rc};

use anyhow::Result;
use sol_logind::{LoginAction, LoginRenderer, LoginService};
use tracing::info;

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("SOL Login Service starting");

    // Check if running in development mode
    let dev_mode = env::args().any(|arg| arg == "--dev" || arg == "--development");

    // Create and start the login service
    let service = if dev_mode {
        info!("Running in DEVELOPMENT mode (mock users, stub auth)");
        Rc::new(RefCell::new(LoginService::new_development()?))
    } else {
        info!("Running in PRODUCTION mode (system users, PAM auth)");
        Rc::new(RefCell::new(LoginService::new()?))
    };

    service.borrow_mut().start()?;

    info!(
        "Login screen initialized with {} users",
        service.borrow().ui.users.len()
    );
    if let Some(user) = service.borrow().ui.selected_user() {
        info!("Default user: {}", user.display_name());
    }

    // Create the Slint renderer
    let renderer = LoginRenderer::new()
        .map_err(|e| anyhow::anyhow!("Failed to create login renderer: {}", e))?;

    // Main event loop
    loop {
        // Render current UI state
        let mode = sol_design::accessibility::TokenMode::light();
        let frame = service.borrow().ui.frame_for(mode);
        renderer.render(&frame);

        info!("Showing login screen - awaiting user interaction");

        // Run UI until action
        let service_clone = Rc::clone(&service);
        let action = renderer
            .run_until_action(
                move |index| {
                    // User selected callback
                    info!("User selected avatar at index {}", index);
                    service_clone.borrow_mut().ui.select_user(index);
                },
                {
                    let service_clone = Rc::clone(&service);
                    move |password| {
                        // Password changed callback
                        service_clone.borrow_mut().ui.set_password(password);
                    }
                },
                {
                    let service_clone = Rc::clone(&service);
                    move || {
                        // Toggle visibility callback
                        service_clone.borrow_mut().ui.toggle_password_visibility();
                    }
                },
                {
                    let _service_clone = Rc::clone(&service);
                    move || {
                        // Login clicked callback
                        info!("Login button clicked");
                    }
                },
                {
                    let service_clone = Rc::clone(&service);
                    move || {
                        // Recompute the frame after any interaction so the
                        // live UI (can-login, selected user, password, etc.)
                        // reflects the latest state.
                        let mode = sol_design::accessibility::TokenMode::light();
                        service_clone.borrow().ui.frame_for(mode)
                    }
                },
            )
            .map_err(|e| anyhow::anyhow!("UI error: {}", e))?;

        match action {
            LoginAction::Authenticated => {
                info!("Attempting authentication...");
                match service.borrow_mut().authenticate() {
                    Ok(token) => {
                        info!("✓ Authentication successful!");
                        info!("  User: {}", token.username);
                        info!("  Session ID: {}", token.session_id);

                        if dev_mode {
                            info!("Development mode: Would spawn compositor + shell here");
                        } else {
                            info!(
                                "Production mode: Would spawn compositor + shell for user session"
                            );
                        }
                        break;
                    }
                    Err(e) => {
                        info!("✗ Authentication failed: {}", e);
                        // TODO: Show error message in UI
                        // Reset UI and loop again
                        service.borrow_mut().ui.reset();
                        continue;
                    }
                }
            }
            LoginAction::Dismissed => {
                info!("Login window dismissed");
                break;
            }
        }
    }

    info!("Login service exiting");
    Ok(())
}
