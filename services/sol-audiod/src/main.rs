use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use sol_audiod::{AudioControl, AudioRouter, Config, PipeWireBackend, RouterConfig, dbus};
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sol_audiod=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("starting sol-audiod");
    let config = Config::load().context("load audio configuration")?;
    let router = AudioRouter::new(RouterConfig {
        auto_switch_headphones: config.routing.auto_switch_headphones,
        auto_switch_speakers: config.routing.auto_switch_speakers,
        auto_switch_wired: config.routing.auto_switch_wired,
        crossfade_duration_ms: config.routing.crossfade_duration_ms,
        detect_shared_usage: config.routing.detect_shared_usage,
        battery_aware: config.routing.battery_aware,
        // Config keys may be Bluetooth addresses while PipeWire IDs include a
        // transport/profile suffix. They are resolved after discovery below.
        priority_boosts: HashMap::new(),
        auto_switch_overrides: HashMap::new(),
    });
    let control = Arc::new(AudioControl::new(
        router,
        Arc::new(PipeWireBackend::default()),
    ));

    match control.refresh() {
        Ok(result) => {
            apply_device_configuration(&control, &config);
            info!(
                connected = result.connected.len(),
                "PipeWire output inventory ready"
            );
        }
        Err(error) => warn!(%error, "PipeWire is not ready; will retry"),
    }

    let (_connection, signals) = dbus::serve_session(Arc::clone(&control))
        .await
        .context("start audio D-Bus service")?;
    info!(
        service = dbus::SERVICE_NAME,
        object = dbus::OBJECT_PATH,
        "sol-audiod control plane ready"
    );

    let mut refresh = tokio::time::interval(Duration::from_secs(2));
    refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    refresh.tick().await;
    loop {
        tokio::select! {
            _ = refresh.tick() => match control.refresh() {
                Ok(result) => {
                    if !result.connected.is_empty() {
                        apply_device_configuration(&control, &config);
                    }
                    if let Err(error) = signals.emit_refresh(&result).await {
                        warn!(%error, "failed to emit audio device signal");
                    }
                }
                Err(error) => warn!(%error, "failed to refresh PipeWire outputs"),
            },
            signal = tokio::signal::ctrl_c() => {
                signal.context("listen for shutdown signal")?;
                info!("shutting down sol-audiod");
                break;
            }
        }
    }
    Ok(())
}

fn apply_device_configuration(control: &AudioControl, config: &Config) {
    let Ok(devices) = control.list_devices() else {
        return;
    };
    for (device, _) in devices {
        for (configured_id, configured) in &config.devices {
            if !ids_match(&device.id, configured_id) {
                continue;
            }
            if let Err(error) = control.set_device_auto_switch(&device.id, configured.auto_switch) {
                warn!(device = %device.id, %error, "failed to apply auto-switch preference");
            }
            if let Err(error) = control.set_device_trusted(&device.id, configured.trusted) {
                warn!(device = %device.id, %error, "failed to apply trusted preference");
            }
        }
        for (configured_id, boost) in &config.routing.priority_boosts {
            if ids_match(&device.id, configured_id)
                && let Err(error) = control.set_device_priority_boost(&device.id, *boost)
            {
                warn!(device = %device.id, %error, "failed to apply priority boost");
            }
        }
    }
}

fn ids_match(backend_id: &str, configured_id: &str) -> bool {
    fn normalized(value: &str) -> String {
        value
            .chars()
            .filter(|character| character.is_ascii_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    }
    let backend = normalized(backend_id);
    let configured = normalized(configured_id);
    !configured.is_empty() && backend.contains(&configured)
}

#[cfg(test)]
mod tests {
    use super::ids_match;

    #[test]
    fn pipewire_bluetooth_id_matches_configured_mac() {
        assert!(ids_match(
            "bluez_output.00_1A_7D_DA_71_13.1",
            "00:1A:7D:DA:71:13"
        ));
        assert!(!ids_match("alsa_output.pci", "00:1A:7D:DA:71:13"));
    }
}
