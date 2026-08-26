//! Layer shell tests and utilities.

#[cfg(test)]
mod tests {
    use crate::scp::{
        capability::{Capability, CapabilityToken, Decision},
        protocol::{ClientMessage, CompositorMessage, LayerShellLayer},
        security::{AppId, AuditOutcome, SecurityCoordinator},
        state::ScpState,
    };
    use std::{collections::HashMap, sync::{Arc, Mutex}};

    #[derive(Default)]
    struct TestSecurity {
        tokens: Mutex<HashMap<Vec<u8>, (AppId, Capability)>>,
    }

    impl SecurityCoordinator for TestSecurity {
        fn verify_app_identity(&self, pid: u32) -> Option<AppId> {
            // Map specific test PIDs to app IDs
            match pid {
                1000 => Some(AppId("sol-shell".to_string())),
                2000 => Some(AppId("regular-app".to_string())),
                _ => Some(AppId(format!("app-{pid}"))),
            }
        }

        fn evaluate_capability(&self, app_id: &AppId, cap: &Capability) -> Decision {
            use crate::scp::capability;

            // Grant default capabilities
            if capability::default_app_capabilities().contains(cap) {
                return Decision::Granted {
                    token: self.issue_token(app_id, cap),
                    expires_at: None,
                };
            }

            // Grant shell-only capabilities to sol-shell
            if capability::shell_only_capabilities().contains(cap) {
                if app_id.0 == "sol-shell" {
                    return Decision::Granted {
                        token: self.issue_token(app_id, cap),
                        expires_at: None,
                    };
                } else {
                    return Decision::Denied {
                        reason: "Reserved for sol-shell".to_string(),
                    };
                }
            }

            // For tests, grant everything else
            Decision::Granted {
                token: self.issue_token(app_id, cap),
                expires_at: None,
            }
        }

        fn issue_token(&self, app_id: &AppId, cap: &Capability) -> CapabilityToken {
            let data = format!("{}:{}", app_id.0, cap.wire_name()).into_bytes();
            self.tokens
                .lock()
                .unwrap()
                .insert(data.clone(), (app_id.clone(), cap.clone()));
            CapabilityToken {
                data,
                expires_at: None,
                one_time: false,
            }
        }

        fn verify_token(&self, token: &CapabilityToken) -> Option<(AppId, Capability)> {
            self.tokens.lock().unwrap().get(&token.data).cloned()
        }

        fn audit_capability_use(&self, _app_id: &AppId, _cap: &Capability, _outcome: AuditOutcome) {
        }
    }

    fn connect_shell(state: &mut ScpState) -> (u64, Vec<u8>) {
        let responses = state
            .handle_message(
                None,
                ClientMessage::Connect {
                    app_id: "sol-shell".to_string(),
                    pid: 1000,
                },
            )
            .expect("shell connects");

        match &responses[0] {
            CompositorMessage::Connected {
                session_id,
                capability_tokens,
                granted_capabilities,
            } => {
                // First, request layer-shell capability if not auto-granted
                if !capability_tokens.contains_key("layer-shell") {
                    let request_responses = state
                        .handle_message(
                            Some(*session_id),
                            ClientMessage::RequestCapability {
                                capability: "layer-shell".to_string(),
                                justification: "Shell requires layer-shell".to_string(),
                            },
                        )
                        .expect("layer-shell requested");

                    match &request_responses[0] {
                        CompositorMessage::CapabilityDecision {
                            granted: true,
                            token: Some(token),
                            ..
                        } => {
                            return (*session_id, token.clone());
                        }
                        msg => panic!("Expected capability grant, got: {:?}", msg),
                    }
                }

                let token = capability_tokens
                    .get("layer-shell")
                    .expect("shell receives layer-shell capability")
                    .clone();
                (*session_id, token)
            }
            _ => panic!("unexpected response"),
        }
    }

    #[test]
    fn layer_shell_granted_to_sol_shell() {
        let mut state = ScpState::with_security(Arc::new(TestSecurity::default()));
        let (session_id, layer_token) = connect_shell(&mut state);

        // Create surface
        state
            .handle_message(
                Some(session_id),
                ClientMessage::CreateSurface { surface_id: 1 },
            )
            .expect("surface created");

        // Create layer surface
        let responses = state
            .handle_message(
                Some(session_id),
                ClientMessage::CreateLayerSurface {
                    surface_id: 1,
                    capability_token: layer_token,
                    layer: LayerShellLayer::Top,
                    namespace: "panel".to_string(),
                    output_id: None,
                },
            )
            .expect("layer surface created");

        assert!(matches!(
            responses[0],
            CompositorMessage::ConfigureLayerSurface { .. }
        ));
    }

    #[test]
    fn layer_shell_denied_to_regular_apps() {
        let mut state = ScpState::with_security(Arc::new(TestSecurity::default()));

        let responses = state
            .handle_message(
                None,
                ClientMessage::Connect {
                    app_id: "regular-app".to_string(),
                    pid: 2000,
                },
            )
            .expect("app connects");

        let session_id = match &responses[0] {
            CompositorMessage::Connected { session_id, capability_tokens, .. } => {
                assert!(!capability_tokens.contains_key("layer-shell"));
                *session_id
            }
            _ => panic!("unexpected response"),
        };

        // Create surface
        state
            .handle_message(
                Some(session_id),
                ClientMessage::CreateSurface { surface_id: 1 },
            )
            .expect("surface created");

        // Try to create layer surface with forged token
        let error = state
            .handle_message(
                Some(session_id),
                ClientMessage::CreateLayerSurface {
                    surface_id: 1,
                    capability_token: b"forged".to_vec(),
                    layer: LayerShellLayer::Top,
                    namespace: "evil".to_string(),
                    output_id: None,
                },
            )
            .expect_err("layer surface rejected");

        assert!(error.contains("not granted") || error.contains("does not match"));
    }

    #[test]
    fn layer_surface_configuration() {
        let mut state = ScpState::with_security(Arc::new(TestSecurity::default()));
        let (session_id, layer_token) = connect_shell(&mut state);

        state
            .handle_message(
                Some(session_id),
                ClientMessage::CreateSurface { surface_id: 1 },
            )
            .expect("surface created");

        let responses = state
            .handle_message(
                Some(session_id),
                ClientMessage::CreateLayerSurface {
                    surface_id: 1,
                    capability_token: layer_token,
                    layer: LayerShellLayer::Bottom,
                    namespace: "dock".to_string(),
                    output_id: None,
                },
            )
            .expect("layer surface created");

        let (layer_id, serial) = match &responses[0] {
            CompositorMessage::ConfigureLayerSurface {
                layer_id,
                serial,
                width,
                height,
            } => {
                assert!(*width > 0);
                assert!(*height > 0);
                (*layer_id, *serial)
            }
            _ => panic!("expected ConfigureLayerSurface"),
        };

        // Set anchor to bottom
        state
            .handle_message(
                Some(session_id),
                ClientMessage::SetLayerAnchor {
                    layer_id,
                    top: false,
                    bottom: true,
                    left: true,
                    right: true,
                },
            )
            .expect("anchor set");

        // Set exclusive zone (reserve 48px for dock)
        state
            .handle_message(
                Some(session_id),
                ClientMessage::SetLayerExclusiveZone {
                    layer_id,
                    zone: 48,
                },
            )
            .expect("exclusive zone set");

        // Set size
        state
            .handle_message(
                Some(session_id),
                ClientMessage::SetLayerSize {
                    layer_id,
                    width: 0,  // stretch horizontally
                    height: 48,
                },
            )
            .expect("size set");

        // Ack configure
        state
            .handle_message(
                Some(session_id),
                ClientMessage::AckLayerConfigure { layer_id, serial },
            )
            .expect("configure acknowledged");
    }

    #[test]
    fn layer_surfaces_sorted_by_layer() {
        let mut state = ScpState::with_security(Arc::new(TestSecurity::default()));
        let (session_id, layer_token) = connect_shell(&mut state);

        // Create multiple layer surfaces
        for (i, layer) in [
            LayerShellLayer::Overlay,
            LayerShellLayer::Background,
            LayerShellLayer::Top,
            LayerShellLayer::Bottom,
        ]
        .iter()
        .enumerate()
        {
            let surface_id = (i + 1) as u32;
            state
                .handle_message(
                    Some(session_id),
                    ClientMessage::CreateSurface { surface_id },
                )
                .expect("surface created");

            state
                .handle_message(
                    Some(session_id),
                    ClientMessage::CreateLayerSurface {
                        surface_id,
                        capability_token: layer_token.clone(),
                        layer: *layer,
                        namespace: format!("layer-{}", i),
                        output_id: None,
                    },
                )
                .expect("layer surface created");
        }

        // Check sorting
        let sorted = state.iter_layer_surfaces_sorted();
        assert_eq!(sorted.len(), 4);

        // Should be ordered: Background < Bottom < Top < Overlay
        assert!(matches!(sorted[0].layer, crate::scp::surface::Layer::Background));
        assert!(matches!(sorted[1].layer, crate::scp::surface::Layer::Bottom));
        assert!(matches!(sorted[2].layer, crate::scp::surface::Layer::Top));
        assert!(matches!(sorted[3].layer, crate::scp::surface::Layer::Overlay));
    }
}
