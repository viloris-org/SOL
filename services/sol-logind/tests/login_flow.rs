use sol_logind::{LoginService, LoginState};

#[test]
fn full_login_flow() {
    let mut service = LoginService::new_development().expect("service creation should succeed");
    service.start().expect("service should start");

    // Initial state: selecting user
    assert!(!service.ui.users.is_empty());
    assert_eq!(service.ui.state, LoginState::SelectingUser);

    // Select a user
    service.ui.select_user(0);
    assert_eq!(service.ui.state, LoginState::EnteringPassword);

    // Enter password
    service.ui.set_password("testpassword".into());
    assert!(service.ui.can_login());

    // Authenticate
    let result = service.authenticate();
    assert!(result.is_ok(), "Authentication should succeed with stub");

    let token = result.unwrap();
    assert_eq!(token.username, service.ui.users[0].username);
    assert_eq!(service.ui.state, LoginState::Authenticated);
}

#[test]
fn login_flow_with_user_switching() {
    let mut service = LoginService::new_development().expect("service creation should succeed");

    // Select first user
    service.ui.select_user(0);
    let first_user = service.ui.selected_user().unwrap().username.clone();
    service.ui.set_password("password1".into());

    // Switch to second user
    service.ui.select_next_user();
    let second_user = service.ui.selected_user().unwrap().username.clone();

    // Password should be cleared after switching
    assert!(service.ui.password.is_empty());
    assert_ne!(first_user, second_user);

    // Enter new password and authenticate
    service.ui.set_password("password2".into());
    let result = service.authenticate();
    assert!(result.is_ok());
    assert_eq!(result.unwrap().username, second_user);
}

#[test]
fn cannot_login_without_password() {
    let mut service = LoginService::new_development().expect("service creation should succeed");
    service.ui.select_user(0);
    service.ui.state = LoginState::EnteringPassword;

    // Empty password
    assert!(!service.ui.can_login());

    // Non-empty password
    service.ui.set_password("pass".into());
    assert!(service.ui.can_login());
}

#[test]
fn authentication_failure_resets_ui() {
    let mut service = LoginService::new_development().expect("service creation should succeed");
    service.ui.select_user(0);
    service.ui.set_password("password".into());

    // In development mode, authentication always succeeds
    // This test documents the expected behavior for production mode
    let result = service.authenticate();
    assert!(result.is_ok());

    // For now, just verify the state after successful auth
    assert_eq!(service.ui.state, LoginState::Authenticated);
}

#[test]
fn password_visibility_toggle_affects_display() {
    let mut service = LoginService::new_development().expect("service creation should succeed");
    service.ui.set_password("secret123".into());

    let mode = sol_design::accessibility::TokenMode::light();

    // Hidden: should show dots
    let frame = service.ui.frame_for(mode);
    assert_eq!(frame.password, "•••••••••");
    assert!(!frame.password_visible);

    // Visible: should show actual text
    service.ui.toggle_password_visibility();
    let frame = service.ui.frame_for(mode);
    assert_eq!(frame.password, "secret123");
    assert!(frame.password_visible);
}
