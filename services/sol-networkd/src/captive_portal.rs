use anyhow::Result;
use tracing::{info, warn};
use std::time::Duration;

/// Captive portal detection and handling
pub struct CaptivePortalDetector {
    check_url: String,
    expected_response: String,
}

impl CaptivePortalDetector {
    pub fn new() -> Self {
        Self {
            check_url: "http://connectivity-check.sol.org/check".to_string(),
            expected_response: "SOL".to_string(),
        }
    }

    pub async fn check_connectivity(&self) -> Result<ConnectivityState> {
        info!("Checking network connectivity");

        // Create HTTP client with timeout
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;

        match client.get(&self.check_url).send().await {
            Ok(response) => {
                // Check for redirect (captive portal)
                if response.status().is_redirection() {
                    info!("Captive portal detected (redirect)");
                    return Ok(ConnectivityState::Portal);
                }

                // Check response body
                if let Ok(body) = response.text().await {
                    if body.trim() == self.expected_response {
                        info!("Full internet connectivity");
                        return Ok(ConnectivityState::Full);
                    } else {
                        info!("Unexpected response, possible captive portal");
                        return Ok(ConnectivityState::Portal);
                    }
                }

                Ok(ConnectivityState::Limited)
            }
            Err(e) => {
                warn!("Connectivity check failed: {}", e);
                Ok(ConnectivityState::None)
            }
        }
    }

    pub async fn get_portal_url(&self) -> Result<Option<String>> {
        info!("Attempting to detect portal URL");

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;

        match client.get(&self.check_url).send().await {
            Ok(response) => {
                if response.status().is_redirection() {
                    if let Some(location) = response.headers().get("Location") {
                        if let Ok(url) = location.to_str() {
                            info!("Portal URL detected: {}", url);
                            return Ok(Some(url.to_string()));
                        }
                    }
                }
                Ok(None)
            }
            Err(_) => Ok(None),
        }
    }

    /// Perform multiple connectivity checks to different endpoints
    pub async fn comprehensive_check(&self) -> Result<ConnectivityState> {
        // Check multiple endpoints for reliability
        let endpoints = vec![
            ("http://connectivity-check.sol.org/check", "SOL"),
            ("http://detectportal.firefox.com/success.txt", "success"),
            ("http://clients3.google.com/generate_204", ""),
        ];

        let mut portal_count = 0;
        let mut success_count = 0;
        let mut fail_count = 0;

        for (url, expected) in endpoints {
            match self.check_endpoint(url, expected).await {
                ConnectivityState::Full => success_count += 1,
                ConnectivityState::Portal => portal_count += 1,
                ConnectivityState::None => fail_count += 1,
                _ => {}
            }
        }

        // Determine overall state
        if success_count >= 2 {
            Ok(ConnectivityState::Full)
        } else if portal_count >= 2 {
            Ok(ConnectivityState::Portal)
        } else if fail_count >= 2 {
            Ok(ConnectivityState::None)
        } else {
            Ok(ConnectivityState::Limited)
        }
    }

    async fn check_endpoint(&self, url: &str, expected: &str) -> ConnectivityState {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();

        match client.get(url).send().await {
            Ok(response) => {
                if response.status().is_redirection() {
                    return ConnectivityState::Portal;
                }

                if expected.is_empty() && response.status() == 204 {
                    return ConnectivityState::Full;
                }

                if let Ok(body) = response.text().await {
                    if body.trim() == expected {
                        return ConnectivityState::Full;
                    }
                }

                ConnectivityState::Portal
            }
            Err(_) => ConnectivityState::None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectivityState {
    None,        // No connection
    Portal,      // Behind captive portal
    Limited,     // Connected but no internet
    Full,        // Full internet connectivity
}
