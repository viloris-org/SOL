//! Stateful audio control plane coordinating policy with the backend.

use std::{
    collections::HashSet,
    sync::{Arc, Mutex, MutexGuard},
};

use thiserror::Error;

use crate::{
    backend::{AudioBackend, BackendError},
    routing::{AudioDevice, AudioRouter},
};

#[derive(Debug, Error)]
pub enum AudioControlError {
    #[error(transparent)]
    Backend(#[from] BackendError),
    #[error("audio routing state lock is poisoned")]
    StatePoisoned,
    #[error("audio output not found: {0}")]
    DeviceNotFound(String),
    #[error("audio output is disconnected: {0}")]
    DeviceDisconnected(String),
    #[error("routing failed: {0}")]
    Routing(String),
}

pub type AudioControlResult<T> = Result<T, AudioControlError>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RefreshResult {
    pub connected: Vec<String>,
    pub disconnected: Vec<String>,
    pub active_change: Option<(Option<String>, Option<String>)>,
}

struct ControlState {
    router: AudioRouter,
    backend_devices: HashSet<String>,
}

/// Thread-safe service core shared by D-Bus handlers and hotplug polling.
pub struct AudioControl {
    backend: Arc<dyn AudioBackend>,
    operation: Mutex<()>,
    state: Mutex<ControlState>,
}

impl AudioControl {
    #[must_use]
    pub fn new(router: AudioRouter, backend: Arc<dyn AudioBackend>) -> Self {
        Self {
            backend,
            operation: Mutex::new(()),
            state: Mutex::new(ControlState {
                router,
                backend_devices: HashSet::new(),
            }),
        }
    }

    fn state(&self) -> AudioControlResult<MutexGuard<'_, ControlState>> {
        self.state
            .lock()
            .map_err(|_| AudioControlError::StatePoisoned)
    }

    fn operation(&self) -> AudioControlResult<MutexGuard<'_, ()>> {
        self.operation
            .lock()
            .map_err(|_| AudioControlError::StatePoisoned)
    }

    /// Reconcile PipeWire inventory and enforce automatic/fallback routing.
    pub fn refresh(&self) -> AudioControlResult<RefreshResult> {
        let _operation = self.operation()?;
        let outputs = self.backend.list_outputs()?;
        let backend_default = outputs
            .iter()
            .find(|output| output.is_default)
            .map(|output| output.id.clone());

        let (mut result, previous_active, desired, manual) = {
            let mut state = self.state()?;
            let old_active = state.router.active_device().map(|device| device.id.clone());
            let new_ids: HashSet<_> = outputs.iter().map(|output| output.id.clone()).collect();
            let mut disconnected: Vec<_> = state
                .backend_devices
                .difference(&new_ids)
                .cloned()
                .collect();
            let mut connected: Vec<_> = new_ids
                .difference(&state.backend_devices)
                .cloned()
                .collect();
            connected.sort();
            disconnected.sort();

            for device_id in &disconnected {
                state.router.unregister_device(device_id);
            }
            for output in outputs {
                state.router.upsert_device(AudioDevice {
                    id: output.id,
                    name: output.name,
                    device_type: output.device_type,
                    is_connected: true,
                    battery_level: None,
                    is_charging: false,
                    last_used: None,
                    trusted: false,
                });
            }
            state.backend_devices = new_ids;

            let old_still_available = old_active
                .as_deref()
                .is_some_and(|id| state.router.device(id).is_some());
            let mut desired = old_active.clone().filter(|_| old_still_available);
            let mut manual = false;

            if desired.is_none() {
                desired = backend_default
                    .clone()
                    .or_else(|| state.router.find_best_device());
            } else if !connected.is_empty() {
                let candidate = connected
                    .iter()
                    .filter_map(|id| state.router.device(id))
                    .filter(|device| state.router.should_auto_switch(device))
                    .max_by_key(|device| state.router.calculate_priority(device))
                    .map(|device| device.id.clone());
                if candidate.is_some() {
                    desired = candidate;
                }
            } else if disconnected.is_empty() && backend_default != old_active {
                // A default change without topology change came from another
                // trusted control surface; preserve it as an explicit choice.
                desired = backend_default.clone();
                manual = true;
            }

            (
                RefreshResult {
                    connected,
                    disconnected,
                    active_change: None,
                },
                old_active,
                desired,
                manual,
            )
        };

        if desired != backend_default
            && let Some(device_id) = &desired
        {
            self.backend.set_default_output(device_id)?;
        }

        let mut state = self.state()?;
        if let Some(device_id) = &desired {
            state
                .router
                .activate_device(device_id, manual)
                .map_err(|error| AudioControlError::Routing(error.to_string()))?;
        }
        let new_active = state.router.active_device().map(|device| device.id.clone());
        if previous_active != new_active {
            result.active_change = Some((previous_active, new_active));
        }
        Ok(result)
    }

    pub fn list_devices(&self) -> AudioControlResult<Vec<(AudioDevice, u8)>> {
        let state = self.state()?;
        let mut devices: Vec<_> = state
            .router
            .devices()
            .filter(|device| device.is_connected)
            .map(|device| (device.clone(), state.router.calculate_priority(device)))
            .collect();
        devices.sort_by(|left, right| {
            right
                .1
                .cmp(&left.1)
                .then_with(|| left.0.id.cmp(&right.0.id))
        });
        Ok(devices)
    }

    pub fn active_device(&self) -> AudioControlResult<Option<(AudioDevice, u8)>> {
        let state = self.state()?;
        Ok(state.router.active_device().map(|device| {
            let priority = state.router.calculate_priority(device);
            (device.clone(), priority)
        }))
    }

    /// Switch the real backend first and commit router state only on success.
    pub fn set_output_device(
        &self,
        device_id: &str,
    ) -> AudioControlResult<(Option<String>, AudioDevice, u8)> {
        let _operation = self.operation()?;
        {
            let state = self.state()?;
            let device = state
                .router
                .device(device_id)
                .ok_or_else(|| AudioControlError::DeviceNotFound(device_id.to_owned()))?;
            if !device.is_connected {
                return Err(AudioControlError::DeviceDisconnected(device_id.to_owned()));
            }
        }

        self.backend.set_default_output(device_id)?;
        let mut state = self.state()?;
        let old = state.router.active_device().map(|device| device.id.clone());
        state
            .router
            .switch_to_device(device_id)
            .map_err(|error| AudioControlError::Routing(error.to_string()))?;
        let device = state
            .router
            .device(device_id)
            .cloned()
            .ok_or_else(|| AudioControlError::DeviceNotFound(device_id.to_owned()))?;
        let priority = state.router.calculate_priority(&device);
        Ok((old, device, priority))
    }

    pub fn set_device_auto_switch(&self, device_id: &str, enabled: bool) -> AudioControlResult<()> {
        self.state()?
            .router
            .set_device_auto_switch(device_id, enabled)
            .map_err(|error| AudioControlError::Routing(error.to_string()))
    }

    pub fn set_device_trusted(&self, device_id: &str, trusted: bool) -> AudioControlResult<()> {
        let mut state = self.state()?;
        if state.router.device(device_id).is_none() {
            return Err(AudioControlError::DeviceNotFound(device_id.to_owned()));
        }
        state.router.set_device_trusted(device_id, trusted);
        Ok(())
    }

    pub fn set_device_priority_boost(&self, device_id: &str, boost: i16) -> AudioControlResult<()> {
        self.state()?
            .router
            .set_device_priority_boost(device_id, boost)
            .map_err(|error| AudioControlError::Routing(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::{AudioDeviceType, backend::BackendOutput, routing::RouterConfig};

    #[derive(Default)]
    struct FakeBackend {
        outputs: Mutex<Vec<BackendOutput>>,
        switches: Mutex<Vec<String>>,
        fail_switch: Mutex<bool>,
    }

    impl FakeBackend {
        fn replace(&self, outputs: Vec<BackendOutput>) {
            *self.outputs.lock().expect("outputs lock") = outputs;
        }
    }

    impl AudioBackend for FakeBackend {
        fn list_outputs(&self) -> crate::backend::BackendResult<Vec<BackendOutput>> {
            Ok(self.outputs.lock().expect("outputs lock").clone())
        }

        fn set_default_output(&self, device_id: &str) -> crate::backend::BackendResult<()> {
            if *self.fail_switch.lock().expect("failure lock") {
                return Err(BackendError::InvalidDeviceId);
            }
            self.switches
                .lock()
                .expect("switches lock")
                .push(device_id.to_owned());
            Ok(())
        }
    }

    fn output(id: &str, kind: AudioDeviceType, is_default: bool) -> BackendOutput {
        BackendOutput {
            id: id.to_owned(),
            name: id.to_owned(),
            device_type: kind,
            is_default,
        }
    }

    fn control(backend: Arc<FakeBackend>) -> AudioControl {
        AudioControl::new(AudioRouter::new(RouterConfig::default()), backend)
    }

    #[test]
    fn initial_refresh_adopts_backend_default() {
        let backend = Arc::new(FakeBackend::default());
        backend.replace(vec![output(
            "speaker",
            AudioDeviceType::BuiltinSpeaker,
            true,
        )]);
        let control = control(backend);
        let result = control.refresh().expect("refresh");
        assert_eq!(result.connected, vec!["speaker"]);
        assert_eq!(result.active_change, Some((None, Some("speaker".into()))));
    }

    #[test]
    fn new_headphones_auto_switch_and_active_removal_falls_back() {
        let backend = Arc::new(FakeBackend::default());
        backend.replace(vec![output(
            "speaker",
            AudioDeviceType::BuiltinSpeaker,
            true,
        )]);
        let control = control(Arc::clone(&backend));
        control.refresh().expect("initial refresh");

        backend.replace(vec![
            output("speaker", AudioDeviceType::BuiltinSpeaker, true),
            output("headphones", AudioDeviceType::Headphones, false),
        ]);
        let connected = control.refresh().expect("headphones refresh");
        assert_eq!(
            connected.active_change.unwrap().1.as_deref(),
            Some("headphones")
        );
        assert_eq!(
            backend.switches.lock().expect("switches").as_slice(),
            ["headphones"]
        );

        backend.replace(vec![output(
            "speaker",
            AudioDeviceType::BuiltinSpeaker,
            false,
        )]);
        let disconnected = control.refresh().expect("fallback refresh");
        assert_eq!(
            disconnected.active_change,
            Some((Some("headphones".into()), Some("speaker".into())))
        );
        assert_eq!(
            backend.switches.lock().expect("switches").last().unwrap(),
            "speaker"
        );
    }

    #[test]
    fn failed_manual_switch_does_not_change_router_state() {
        let backend = Arc::new(FakeBackend::default());
        backend.replace(vec![
            output("speaker", AudioDeviceType::BuiltinSpeaker, true),
            output("headphones", AudioDeviceType::Headphones, false),
        ]);
        let control = control(Arc::clone(&backend));
        control.refresh().expect("initial refresh");
        *backend.fail_switch.lock().expect("failure lock") = true;
        assert!(control.set_output_device("headphones").is_err());
        assert_eq!(
            control.active_device().expect("active").unwrap().0.id,
            "speaker"
        );
    }

    #[test]
    fn removing_only_output_reports_transition_to_none() {
        let backend = Arc::new(FakeBackend::default());
        backend.replace(vec![output(
            "speaker",
            AudioDeviceType::BuiltinSpeaker,
            true,
        )]);
        let control = control(Arc::clone(&backend));
        control.refresh().expect("initial refresh");
        backend.replace(Vec::new());

        let result = control.refresh().expect("empty refresh");
        assert_eq!(result.active_change, Some((Some("speaker".into()), None)));
        assert!(control.active_device().expect("active query").is_none());
    }
}
